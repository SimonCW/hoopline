//! Device registration and subscription validation.
//!
mod queue;
mod sender;

use axum::{
    Extension, Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, header},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{
    Brand,
    error::{AppError, Problem},
    i18n::{Language, Locale},
    identity::{BookingIdentity, cookie_value, current_user_from_headers, user_id_from_cookie},
};

const MAX_DEVICES_PER_MEMBER: i64 = 5;
const MAX_STORED_DEVICES: i64 = 5_000;

async fn device_capacity_available(
    tx: &mut Transaction<'_, Sqlite>,
    hash: &[u8; 32],
) -> Result<bool, sqlx::Error> {
    let (exists, count): (bool, i64) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM push_devices WHERE token_hash = ?),
                (SELECT count(*) FROM push_devices)",
    )
    .bind(hash.as_slice())
    .fetch_one(&mut **tx)
    .await?;
    Ok(exists || count < MAX_STORED_DEVICES)
}

#[derive(Clone, Default)]
pub(crate) struct Config {
    // None means push is disabled.
    pub(crate) origin: Option<String>,
    pub(crate) public_key: String,
    pub(crate) secure_cookie: bool,
}

pub(crate) fn initialize(pool: &SqlitePool, brand: Brand) -> Result<Config, AppError> {
    let enabled = std::env::var("PUSH_ENABLED").unwrap_or_else(|_| "false".into());
    if enabled == "false" {
        return Ok(Config::default());
    }
    if enabled != "true" {
        return Err(AppError::Configuration(
            "PUSH_ENABLED must be true or false",
        ));
    }
    let origin = configured_origin(
        std::env::var("APP_ORIGIN").ok().as_deref(),
        std::env::var("BUILTIN_DOMAIN").ok().as_deref(),
    )?;
    let public = std::env::var("VAPID_PUBLIC_KEY")
        .map_err(|_| AppError::Configuration("VAPID_PUBLIC_KEY is required"))?;
    let private = std::env::var("VAPID_PRIVATE_KEY")
        .map_err(|_| AppError::Configuration("VAPID_PRIVATE_KEY is required"))?;
    let subject = std::env::var("VAPID_SUBJECT")
        .map_err(|_| AppError::Configuration("VAPID_SUBJECT is required"))?;
    let (config, sender) = configuration(&origin, &public, &private, &subject)?;
    tokio::spawn(sender::run(pool.clone(), sender, brand));
    Ok(config)
}

fn configured_origin(
    app_origin: Option<&str>,
    builtin_domain: Option<&str>,
) -> Result<String, AppError> {
    if let Some(origin) = app_origin {
        return Ok(origin.to_owned());
    }
    if let Some(domain) = builtin_domain {
        match url::Host::parse(domain) {
            Ok(url::Host::Domain(parsed))
                if parsed == domain && parsed.contains('.') && !parsed.ends_with('.') =>
            {
                return Ok(format!("https://{parsed}"));
            }
            _ => return Err(AppError::Configuration("invalid BUILTIN_DOMAIN")),
        }
    }
    Err(AppError::Configuration(
        "APP_ORIGIN is required when push is enabled",
    ))
}

fn configuration(
    origin: &str,
    public: &str,
    private: &str,
    subject: &str,
) -> Result<(Config, sender::Sender), AppError> {
    let parsed =
        url::Url::parse(origin).map_err(|_| AppError::Configuration("invalid APP_ORIGIN"))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.origin().ascii_serialization() != origin
    {
        return Err(AppError::Configuration(
            "APP_ORIGIN must be a canonical HTTPS origin",
        ));
    }
    let sender = sender::Sender::new(private, public, subject).map_err(AppError::Configuration)?;
    Ok((
        Config {
            origin: Some(origin.into()),
            public_key: public.into(),
            secure_cookie: true,
        },
        sender,
    ))
}

fn device_token(value: &str) -> Option<(&str, [u8; 32], i64)> {
    let (token, revision) = value.split_once('.').unwrap_or((value, "0"));
    if token.len() != 43 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(token).ok()?;
    let revision: i64 = revision.parse().ok()?;
    (decoded.len() == 32 && revision >= 0)
        .then(|| (token, Sha256::digest(decoded).into(), revision))
}

pub(crate) fn device_cookie(headers: &HeaderMap, config: &Config) -> Option<HeaderValue> {
    if config.origin.is_none()
        || cookie_value(headers, "push_device")
            .and_then(device_token)
            .is_some()
    {
        return None;
    }
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    device_cookie_header(&URL_SAFE_NO_PAD.encode(bytes), config.secure_cookie)
}

fn device_cookie_header(token: &str, secure: bool) -> Option<HeaderValue> {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "push_device={token}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax{secure}"
    ))
    .ok()
}

fn owned_token(headers: &HeaderMap) -> Result<[u8; 32], AppError> {
    cookie_value(headers, "push_device")
        .and_then(device_token)
        .map(|(_, hash, _)| hash)
        .ok_or(AppError::Forbidden(Problem::DeviceRequired))
}

pub(crate) async fn detach(
    tx: &mut Transaction<'_, Sqlite>,
    headers: &HeaderMap,
    config: &Config,
) -> Result<Option<HeaderValue>, sqlx::Error> {
    let Some((token, hash, _)) = cookie_value(headers, "push_device").and_then(device_token) else {
        return Ok(None);
    };
    if !device_capacity_available(tx, &hash).await? {
        return Ok(None);
    }
    let now = crate::clock::now().timestamp();
    // Use the stable token on identity changes. A lost response can leave a
    // stale revision in the browser, but logout must still detach the device.
    let next: Option<i64> = sqlx::query_scalar(
        "INSERT INTO push_devices (token_hash, created_at, updated_at, identity_revision)
         SELECT ?, ?, ?, 1 WHERE ? OR EXISTS (
             SELECT 1 FROM push_devices WHERE token_hash = ?
         ) ON CONFLICT(token_hash) DO UPDATE SET
             user_id = NULL, generation = generation + (user_id IS NOT NULL),
             updated_at = excluded.updated_at, identity_revision = identity_revision + 1
         RETURNING identity_revision",
    )
    .bind(hash.as_slice())
    .bind(now)
    .bind(now)
    .bind(config.origin.is_some())
    .bind(hash.as_slice())
    .fetch_optional(&mut **tx)
    .await?;
    let Some(next) = next else {
        return Ok(None);
    };
    sqlx::query(
        "UPDATE push_jobs SET status = 'skipped', completed_at = ?
         WHERE device_id = (SELECT id FROM push_devices WHERE token_hash = ?)
         AND status IN ('pending', 'claimed')",
    )
    .bind(now)
    .bind(hash.as_slice())
    .execute(&mut **tx)
    .await?;
    Ok(device_cookie_header(
        &format!("{token}.{next}"),
        config.secure_cookie || config.origin.is_none(),
    ))
}

pub(crate) async fn update_language(
    pool: &SqlitePool,
    headers: &HeaderMap,
    locale: Locale,
) -> Result<(), sqlx::Error> {
    if let Some((_, hash, revision)) = cookie_value(headers, "push_device").and_then(device_token)
        && let Some(user_id) = user_id_from_cookie(headers)
    {
        sqlx::query("UPDATE push_devices SET locale = ? WHERE token_hash = ? AND user_id = ? AND identity_revision = ?")
            .bind(locale.code())
            .bind(hash.as_slice())
            .bind(user_id)
            .bind(revision)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub(crate) fn check_origin(headers: &HeaderMap, config: &Config) -> Result<(), AppError> {
    let origin = config
        .origin
        .as_deref()
        .ok_or(AppError::Forbidden(Problem::AlertsUnavailable))?;
    if headers.get_all(header::ORIGIN).iter().count() != 1
        || headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            != Some(origin)
    {
        return Err(AppError::Forbidden(Problem::BadOrigin));
    }
    Ok(())
}

#[derive(Serialize)]
pub(crate) struct Status {
    available: bool,
    public_key: String,
    state: &'static str,
}

pub(crate) async fn status(
    State(pool): State<SqlitePool>,
    Extension(config): Extension<Config>,
    headers: HeaderMap,
) -> Result<Json<Status>, AppError> {
    let user = current_user_from_headers(&pool, &headers).await?;
    let state = device_state(&pool, &config, &headers, user.map(|user| user.id)).await?;
    Ok(Json(Status {
        available: config.origin.is_some(),
        public_key: config.public_key.clone(),
        state,
    }))
}

pub(crate) async fn device_state(
    pool: &SqlitePool,
    config: &Config,
    headers: &HeaderMap,
    member: Option<i64>,
) -> Result<&'static str, sqlx::Error> {
    if config.origin.is_none() {
        return Ok("unavailable");
    }
    let Some((_, hash, revision)) = cookie_value(headers, "push_device").and_then(device_token)
    else {
        return Ok("unregistered");
    };
    let row = sqlx::query(
        "SELECT user_id, enabled, endpoint, identity_revision FROM push_devices WHERE token_hash = ?",
    )
    .bind(hash.as_slice())
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(row) if row.get::<i64, _>("identity_revision") != revision => "stale",
        Some(row) if row.get::<i64, _>("enabled") == 0 => "disabled",
        Some(row) if member.is_some() && row.get::<Option<i64>, _>("user_id") == member => {
            if row.get::<Option<String>, _>("endpoint").is_some() {
                "registered"
            } else {
                "expired"
            }
        }
        None if revision != 0 => "stale",
        _ => "unregistered",
    })
}

#[derive(serde::Deserialize)]
pub(crate) struct SubscribeQuery {
    #[serde(default)]
    automatic: bool,
}

pub(crate) async fn subscribe(
    State(pool): State<SqlitePool>,
    lang: Language,
    Extension(config): Extension<Config>,
    Query(query): Query<SubscribeQuery>,
    user: BookingIdentity,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Status>, AppError> {
    check_origin(&headers, &config)?;
    let token = cookie_value(&headers, "push_device")
        .ok_or(AppError::Forbidden(Problem::DeviceRequired))?;
    let body =
        std::str::from_utf8(&body).map_err(|_| AppError::BadRequest(Problem::BadSubscription))?;
    let binding = Binding::parse(token, body, lang.locale)
        .ok_or(AppError::BadRequest(Problem::BadSubscription))?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if bind_with_preference(&mut tx, user.id(), &binding, !query.automatic).await?
        != BindOutcome::Registered
    {
        return Err(AppError::BadRequest(Problem::AlertsFailed));
    }
    tx.commit().await?;
    Ok(Json(Status {
        available: true,
        public_key: config.public_key,
        state: "registered",
    }))
}

pub(crate) async fn disable(
    State(pool): State<SqlitePool>,
    Extension(config): Extension<Config>,
    headers: HeaderMap,
) -> Result<Json<Status>, AppError> {
    check_origin(&headers, &config)?;
    let hash = owned_token(&headers)?;
    let now = crate::clock::now().timestamp();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if !device_capacity_available(&mut tx, &hash).await? {
        return Err(AppError::BadRequest(Problem::DeviceStorageFull));
    }
    sqlx::query(
        "INSERT INTO push_devices (token_hash, enabled, created_at, updated_at)
         VALUES (?, 0, ?, ?) ON CONFLICT(token_hash) DO UPDATE SET
         generation = generation + CASE WHEN enabled = 1 THEN 1 ELSE 0 END,
         enabled = 0, updated_at = excluded.updated_at",
    )
    .bind(hash.as_slice())
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE push_jobs SET status = 'skipped', completed_at = ?
         WHERE device_id = (SELECT id FROM push_devices WHERE token_hash = ?)
         AND status IN ('pending', 'claimed')",
    )
    .bind(now)
    .bind(hash.as_slice())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(Status {
        available: true,
        public_key: config.public_key,
        state: "disabled",
    }))
}

pub(crate) async fn send_test(
    State(pool): State<SqlitePool>,
    Extension(config): Extension<Config>,
    user: BookingIdentity,
    headers: HeaderMap,
) -> Result<Json<Status>, AppError> {
    check_origin(&headers, &config)?;
    let hash = owned_token(&headers)?;
    let now = crate::clock::now().timestamp();
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let device: Option<(i64, i64)> = sqlx::query_as(
        "SELECT id, generation FROM push_devices
         WHERE token_hash = ? AND user_id = ? AND enabled = 1 AND endpoint IS NOT NULL",
    )
    .bind(hash.as_slice())
    .bind(user.id())
    .fetch_optional(&mut *tx)
    .await?;
    let (device_id, generation) = device.ok_or(AppError::BadRequest(Problem::EnableAlertsFirst))?;
    let (minute, hour): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE created_at > ? - 60), count(*)
         FROM push_jobs WHERE kind = 'test' AND device_id = ? AND created_at > ? - 3600",
    )
    .bind(now)
    .bind(device_id)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    if minute >= 1 || hour >= 5 {
        return Err(AppError::RateLimited);
    }
    sqlx::query(
        "INSERT INTO push_jobs (kind, device_id, generation, user_id, created_at, next_attempt_at, expires_at)
         VALUES ('test', ?, ?, ?, ?, ?, ?)",
    )
    .bind(device_id)
    .bind(generation)
    .bind(user.id())
    .bind(now)
    .bind(now)
    .bind(now + 1800)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(Status {
        available: true,
        public_key: config.public_key,
        state: "registered",
    }))
}

#[derive(Deserialize)]
struct SubscriptionKeys {
    p256dh: String,
    auth: String,
}

#[derive(Deserialize)]
struct Subscription {
    endpoint: String,
    keys: SubscriptionKeys,
}

/// A validated browser subscription and the hash of its ownership token.
pub(crate) struct Binding {
    locale: Locale,
    token_hash: [u8; 32],
    identity_revision: i64,
    endpoint: String,
    p256dh: String,
    auth: String,
}

impl Binding {
    // The token is issued by the server and must be stored in an HttpOnly cookie.
    pub(crate) fn parse(token: &str, json: &str, locale: Locale) -> Option<Self> {
        if json.len() > 4096 {
            return None;
        }
        let (_, token_hash, identity_revision) = device_token(token)?;
        let subscription: Subscription = serde_json::from_str(json).ok()?;
        if !valid_endpoint(&subscription.endpoint)
            || subscription.keys.p256dh.len() != 87
            || subscription.keys.auth.len() != 22
        {
            return None;
        }
        let public_key = URL_SAFE_NO_PAD.decode(&subscription.keys.p256dh).ok()?;
        let auth = URL_SAFE_NO_PAD.decode(&subscription.keys.auth).ok()?;
        if public_key.len() != 65
            || public_key.first() != Some(&4)
            || p256::PublicKey::from_sec1_bytes(&public_key).is_err()
            || auth.len() != 16
        {
            return None;
        }
        Some(Self {
            locale,
            token_hash,
            identity_revision,
            endpoint: subscription.endpoint,
            p256dh: subscription.keys.p256dh,
            auth: subscription.keys.auth,
        })
    }
}

fn valid_endpoint(endpoint: &str) -> bool {
    if endpoint.len() > 2048 {
        return false;
    }
    let Ok(url) = url::Url::parse(endpoint) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let Some(url::Host::Domain(host)) = url.host() else {
        return false;
    };
    host == "fcm.googleapis.com"
        || host == "updates.push.services.mozilla.com"
        || host == "web.push.apple.com"
        || host
            .strip_suffix(".push.apple.com")
            .is_some_and(|prefix| !prefix.is_empty())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindOutcome {
    Registered,
    Unavailable,
}

/// Bind a device while holding the caller's immediate transaction.
pub(crate) async fn bind(
    tx: &mut Transaction<'_, Sqlite>,
    member_id: i64,
    binding: &Binding,
) -> Result<BindOutcome, sqlx::Error> {
    bind_with_preference(tx, member_id, binding, false).await
}

async fn bind_with_preference(
    tx: &mut Transaction<'_, Sqlite>,
    member_id: i64,
    binding: &Binding,
    explicit_enable: bool,
) -> Result<BindOutcome, sqlx::Error> {
    if !device_capacity_available(tx, &binding.token_hash).await? {
        return Ok(BindOutcome::Unavailable);
    }
    let existing = sqlx::query(
        "SELECT id, user_id, endpoint, p256dh, auth, enabled, identity_revision
         FROM push_devices WHERE token_hash = ?",
    )
    .bind(binding.token_hash.as_slice())
    .fetch_optional(&mut **tx)
    .await?;
    if existing
        .as_ref()
        .map_or(0, |row| row.get::<i64, _>("identity_revision"))
        != binding.identity_revision
    {
        return Ok(BindOutcome::Unavailable);
    }
    let existing_id = existing.as_ref().map(|row| row.get::<i64, _>("id"));
    if existing
        .as_ref()
        .is_some_and(|row| row.get::<i64, _>("enabled") == 0)
        && !explicit_enable
    {
        return Ok(BindOutcome::Unavailable);
    }
    let endpoint_owner: Option<i64> =
        sqlx::query_scalar("SELECT id FROM push_devices WHERE endpoint = ?")
            .bind(&binding.endpoint)
            .fetch_optional(&mut **tx)
            .await?;
    if endpoint_owner.is_some() && endpoint_owner != existing_id {
        return Ok(BindOutcome::Unavailable);
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM push_devices WHERE user_id = ? AND enabled = 1
         AND endpoint IS NOT NULL AND id != COALESCE(?, -1)",
    )
    .bind(member_id)
    .bind(existing_id)
    .fetch_one(&mut **tx)
    .await?;
    if count >= MAX_DEVICES_PER_MEMBER {
        return Ok(BindOutcome::Unavailable);
    }
    let now = crate::clock::now().timestamp();
    if let Some(row) = existing {
        let id: i64 = row.try_get("id")?;
        let changed = row.try_get::<i64, _>("enabled")? == 0
            || row.try_get::<Option<i64>, _>("user_id")? != Some(member_id)
            || row.try_get::<Option<String>, _>("endpoint")?.as_deref()
                != Some(binding.endpoint.as_str())
            || row.try_get::<Option<String>, _>("p256dh")?.as_deref()
                != Some(binding.p256dh.as_str())
            || row.try_get::<Option<String>, _>("auth")?.as_deref() != Some(binding.auth.as_str());
        if changed {
            sqlx::query(
                "UPDATE push_jobs SET status = 'skipped', completed_at = ?
                 WHERE device_id = ? AND status IN ('pending', 'claimed')",
            )
            .bind(now)
            .bind(id)
            .execute(&mut **tx)
            .await?;
        }
        // A delayed enrollment must not undo a more recent language switch.
        sqlx::query(
            "UPDATE push_devices SET user_id = ?1, endpoint = ?2, p256dh = ?3, auth = ?4, enabled = 1,
             locale = CASE WHEN user_id = ?1 THEN COALESCE(locale, ?5) ELSE ?5 END,
             generation = generation + ?6, updated_at = ?7 WHERE id = ?8",
        )
        .bind(member_id)
        .bind(&binding.endpoint)
        .bind(&binding.p256dh)
        .bind(&binding.auth)
        .bind(binding.locale.code())
        .bind(i64::from(changed))
        .bind(now)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO push_devices (token_hash, user_id, endpoint, p256dh, auth, locale, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(binding.token_hash.as_slice())
        .bind(member_id)
        .bind(&binding.endpoint)
        .bind(&binding.p256dh)
        .bind(&binding.auth)
        .bind(binding.locale.code())
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(BindOutcome::Registered)
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode, header},
    };
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use http_body_util::BodyExt;
    use p256::elliptic_curve::sec1::ToEncodedPoint;
    use serde_json::json;
    use tower::ServiceExt;

    use super::{BindOutcome, Binding};
    use crate::db::{SignupOutcome, cancel_booking_for_slot, init_pool, signup_with_push};

    fn token(byte: u8) -> String {
        URL_SAFE_NO_PAD.encode([byte; 32])
    }

    fn subscription(endpoint: &str) -> String {
        let key = p256::SecretKey::from_slice(&[1; 32]).unwrap().public_key();
        json!({
            "endpoint": endpoint,
            "keys": {
                "p256dh": URL_SAFE_NO_PAD.encode(key.to_encoded_point(false).as_bytes()),
                "auth": URL_SAFE_NO_PAD.encode([2; 16])
            }
        })
        .to_string()
    }

    fn binding(byte: u8, endpoint: &str) -> Binding {
        Binding::parse(&token(byte), &subscription(endpoint), super::Locale::En).unwrap()
    }

    async fn fixture() -> sqlx::SqlitePool {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, name) VALUES (1, 'Player'), (2, 'Waiter'), (3, 'Other');
             INSERT INTO slots (id, datetime, venue, max_players, max_waitlist)
             VALUES (1, '2099-01-01T19:00:00Z', 'Court', 1, 2);
             INSERT INTO bookings (slot_id, user_id, position, is_waitlist)
             VALUES (1, 1, 1, 0);",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn router(pool: sqlx::SqlitePool) -> axum::Router {
        crate::app_with_push_config(
            pool,
            super::Config {
                origin: Some("https://hoopline.example".into()),
                public_key: "public-key".into(),
                secure_cookie: true,
            },
            crate::Brand::default(),
        )
    }

    #[test]
    fn startup_configuration_requires_matching_keys_and_canonical_https_origin() {
        let private = "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY";
        let public = URL_SAFE_NO_PAD.encode(
            web_push::VapidSignatureBuilder::from_base64_no_sub(private)
                .unwrap()
                .get_public_key(),
        );
        assert!(
            super::configuration(
                "https://hoopline.example",
                &public,
                private,
                "mailto:admin@example.com"
            )
            .is_ok()
        );
        assert!(
            super::configuration(
                "http://hoopline.example",
                &public,
                private,
                "mailto:admin@example.com"
            )
            .is_err()
        );
        assert!(
            super::configuration(
                "https://hoopline.example/path",
                &public,
                private,
                "mailto:admin@example.com"
            )
            .is_err()
        );
        assert!(
            super::configuration(
                "https://hoopline.example",
                "wrong",
                private,
                "mailto:admin@example.com"
            )
            .is_err()
        );
        assert!(
            super::configuration("https://hoopline.example", &public, private, "invalid").is_err()
        );
    }

    #[test]
    fn hostim_origin_uses_trusted_builtin_domain_when_not_explicitly_configured() {
        assert_eq!(
            super::configured_origin(None, Some("hoopline-example.hostim.app")).unwrap(),
            "https://hoopline-example.hostim.app"
        );
        assert_eq!(
            super::configured_origin(
                Some("https://basketball.example"),
                Some("hoopline-example.hostim.app"),
            )
            .unwrap(),
            "https://basketball.example"
        );
        assert!(super::configured_origin(None, None).is_err());
        for invalid in [
            "",
            "localhost",
            "127.0.0.1",
            "example.com:443",
            "https://example.com",
            "example.com/path",
            "user@example.com",
            "example.com.",
        ] {
            assert!(
                super::configured_origin(None, Some(invalid)).is_err(),
                "{invalid}"
            );
        }
    }

    fn request(method: &str, path: &str, cookie: &str, origin: &str, body: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, cookie)
            .header(header::ORIGIN, origin)
            .header("x-hoopline-user", "2")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    #[tokio::test]
    async fn overview_issues_a_secure_ownership_cookie_only_when_needed() {
        let app = router(fixture().await);
        let response = app
            .clone()
            .oneshot(request("GET", "/slots", "user_id=2", "", ""))
            .await
            .unwrap();
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(cookie.starts_with("push_device="));
        assert!(cookie.contains("; HttpOnly; SameSite=Lax; Secure"));
        let token = cookie.split(';').next().unwrap();
        let response = app
            .oneshot(request(
                "GET",
                "/slots",
                &format!("user_id=2; {token}"),
                "",
                "",
            ))
            .await
            .unwrap();
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }

    #[tokio::test]
    async fn registration_rejects_cross_origin_and_stale_identity() {
        let pool = fixture().await;
        let app = router(pool.clone());
        let cookie = format!("user_id=2; push_device={}", token(1));
        let body = subscription("https://fcm.googleapis.com/first");
        let cross = app
            .clone()
            .oneshot(request(
                "POST",
                "/push/subscribe",
                &cookie,
                "https://evil.example",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(cross.status(), StatusCode::FORBIDDEN);
        let mut stale = request(
            "POST",
            "/push/subscribe",
            &cookie,
            "https://hoopline.example",
            &body,
        );
        stale
            .headers_mut()
            .insert("x-hoopline-user", "3".parse().unwrap());
        let stale = app.oneshot(stale).await.unwrap();
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn waitlist_signup_registers_only_with_same_origin_and_owned_cookie() {
        let pool = fixture().await;
        let app = router(pool.clone());
        let cookie = format!("user_id=2; push_device={}", token(1));
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "push_subscription",
                &subscription("https://fcm.googleapis.com/first"),
            )
            .finish();
        let response = app
            .oneshot(request(
                "POST",
                "/slots/1/signup",
                &cookie,
                "https://hoopline.example",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("Promotion alerts are on"));
        let binding: (i64, i64) = sqlx::query_as(
            "SELECT d.user_id, b.is_waitlist FROM push_devices d JOIN bookings b ON b.user_id = d.user_id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(binding, (2, 1));
    }

    #[tokio::test]
    async fn device_language_follows_owned_preference_without_resetting_jobs() {
        let pool = fixture().await;
        let app = router(pool.clone());
        let cookie = format!("user_id=2; push_device={}; hoopline_lang=de", token(1));
        let subscription = subscription("https://fcm.googleapis.com/first");
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("push_subscription", &subscription)
            .finish();
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/slots/1/signup",
                &cookie,
                "https://hoopline.example",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let html = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&html).contains("Warteliste: Platz 1"));
        let original: (String, i64) = sqlx::query_as("SELECT locale, generation FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(original.0, "de");

        sqlx::query("UPDATE push_devices SET identity_revision = 2")
            .execute(&pool)
            .await
            .unwrap();
        for (user, revision, expected) in [(3, 2, "de"), (2, 1, "de"), (2, 2, "en")] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/language")
                        .header(
                            header::COOKIE,
                            format!("user_id={user}; push_device={}.{revision}", token(1)),
                        )
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .body(Body::from("locale=en&return_to=%2Fslots"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            let actual: (String, i64) =
                sqlx::query_as("SELECT locale, generation FROM push_devices")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(actual, (expected.to_owned(), original.1));
        }

        let response = app
            .oneshot(request(
                "POST",
                "/push/subscribe?automatic=true",
                &format!("user_id=2; push_device={}.2; hoopline_lang=de", token(1)),
                "https://hoopline.example",
                &subscription,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let actual: (String, i64) = sqlx::query_as("SELECT locale, generation FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(actual, ("en".to_owned(), original.1));
    }

    #[tokio::test]
    async fn invalid_optional_signup_binding_saves_booking_with_separate_feedback() {
        let pool = fixture().await;
        let cookie = format!("user_id=2; push_device={}", token(1));
        let body = url::form_urlencoded::Serializer::new(String::new())
            .append_pair(
                "push_subscription",
                &subscription("https://fcm.googleapis.com/first"),
            )
            .finish();
        let response = router(pool.clone())
            .oneshot(request(
                "POST",
                "/slots/1/signup",
                &cookie,
                "https://evil.example",
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("Booking saved."));
        assert!(body.contains("Alerts could not be enabled"));
        let counts: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM bookings WHERE user_id = 2),
                    (SELECT count(*) FROM push_devices)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counts, (1, 0));
    }

    #[tokio::test]
    async fn disabled_production_router_rejects_registration_and_issues_no_device_cookie() {
        let pool = fixture().await;
        let app = crate::app_with_pool(pool);
        let cookie = format!("user_id=2; push_device={}", token(1));
        let response = app
            .clone()
            .oneshot(request("GET", "/slots", "user_id=2", "", ""))
            .await
            .unwrap();
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        let response = app
            .oneshot(request(
                "POST",
                "/push/subscribe",
                &cookie,
                "https://hoopline.example",
                &subscription("https://fcm.googleapis.com/first"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn registration_disable_and_explicit_reenable_are_owned_and_idempotent() {
        let pool = fixture().await;
        let app = router(pool.clone());
        let cookie = format!("user_id=2; push_device={}", token(1));
        let body = subscription("https://fcm.googleapis.com/first");
        let origin = "https://hoopline.example";
        let response = app
            .clone()
            .oneshot(request("POST", "/push/subscribe", &cookie, origin, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(request("GET", "/push/status", &cookie, origin, ""))
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let status: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status["state"], "registered");
        assert!(!String::from_utf8_lossy(&bytes).contains("fcm.googleapis.com"));
        let thief = format!("user_id=2; push_device={}", token(2));
        let response = app
            .clone()
            .oneshot(request("POST", "/push/subscribe", &thief, origin, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(request("POST", "/push/disable", &cookie, origin, ""))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
        let generation: i64 = sqlx::query_scalar("SELECT generation FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(generation, 1);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/push/subscribe?automatic=true",
                &cookie,
                origin,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = app
            .oneshot(request("POST", "/push/subscribe", &cookie, origin, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let device: (i64, i64) = sqlx::query_as("SELECT enabled, generation FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(device, (1, 2));
    }

    #[tokio::test]
    async fn test_alert_requires_owned_registration_and_is_rate_limited() {
        let pool = fixture().await;
        let app = router(pool.clone());
        let cookie = format!("user_id=2; push_device={}", token(1));
        let origin = "https://hoopline.example";
        let body = subscription("https://fcm.googleapis.com/first");
        let response = app
            .clone()
            .oneshot(request("POST", "/push/subscribe", &cookie, origin, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let thief = format!("user_id=2; push_device={}", token(2));
        let response = app
            .clone()
            .oneshot(request("POST", "/push/test", &thief, origin, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let response = app
            .clone()
            .oneshot(request("POST", "/push/test", &cookie, origin, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .oneshot(request("POST", "/push/test", &cookie, origin, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_jobs WHERE kind = 'test'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn logout_detaches_only_owned_device_and_invalidates_jobs_even_when_push_is_off() {
        let pool = fixture().await;
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        for n in 1..=2 {
            let device = binding(n, &format!("https://fcm.googleapis.com/{n}"));
            super::bind(&mut tx, 2, &device).await.unwrap();
        }
        sqlx::raw_sql(
            "UPDATE push_devices SET enabled = 0 WHERE id = 1;
             INSERT INTO push_jobs (kind, device_id, generation, user_id, created_at, next_attempt_at, expires_at)
             VALUES ('test', 1, 0, 2, 0, 0, 9999999999),
                    ('test', 2, 0, 2, 0, 0, 9999999999);",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let cookie = format!("user_id=2; push_device={}", token(1));
        let response = crate::app_with_pool(pool.clone())
            .oneshot(request("POST", "/users/logout", &cookie, "", ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let devices: Vec<(Option<i64>, i64, i64)> =
            sqlx::query_as("SELECT user_id, enabled, generation FROM push_devices ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(devices, vec![(None, 0, 1), (Some(2), 1, 0)]);
        let jobs: Vec<String> = sqlx::query_scalar("SELECT status FROM push_jobs ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, vec!["skipped", "pending"]);
    }

    #[tokio::test]
    async fn logout_fences_old_enrollment_even_before_first_registration() {
        let pool = fixture().await;
        let app = router(pool.clone());
        let old_cookie = format!("user_id=2; push_device={}", token(1));
        let origin = "https://hoopline.example";
        let body = subscription("https://fcm.googleapis.com/first");
        let logout = app
            .clone()
            .oneshot(request("POST", "/users/logout", &old_cookie, "", ""))
            .await
            .unwrap();
        let new_device = format!("push_device={}.1", token(1));
        assert!(
            logout
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .any(|cookie| cookie.to_str().unwrap().starts_with(&new_device))
        );
        let stale = app
            .clone()
            .oneshot(request(
                "POST",
                "/push/subscribe?automatic=true",
                &old_cookie,
                origin,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::BAD_REQUEST);
        let old_binding = Binding::parse(&token(1), &body, super::Locale::En).unwrap();
        assert_eq!(
            signup_with_push(&pool, 1, 2, Some(&old_binding))
                .await
                .unwrap(),
            (SignupOutcome::AddedWaitlist, Some(BindOutcome::Unavailable))
        );
        let new_cookie = format!("user_id=3; {new_device}");
        let mut enrollment = request("POST", "/push/subscribe", &new_cookie, origin, &body);
        enrollment
            .headers_mut()
            .insert("x-hoopline-user", "3".parse().unwrap());
        let fresh = app.clone().oneshot(enrollment).await.unwrap();
        assert_eq!(fresh.status(), StatusCode::OK);
        let owner: i64 = sqlx::query_scalar("SELECT user_id FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(owner, 3);

        // Simulate an identity change after the browser did not retain the
        // reissued revision. It must still detach the current binding.
        let stale_cookie = format!("user_id=3; push_device={}", token(1));
        let logout = app
            .oneshot(request("POST", "/users/logout", &stale_cookie, "", ""))
            .await
            .unwrap();
        assert_eq!(logout.status(), StatusCode::SEE_OTHER);
        assert!(
            logout
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .any(|cookie| cookie
                    .to_str()
                    .unwrap()
                    .starts_with(&format!("push_device={}.2;", token(1))))
        );
        let owner: Option<i64> = sqlx::query_scalar("SELECT user_id FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(owner, None);
    }

    #[test]
    fn validates_endpoint_and_subscription_keys() {
        for endpoint in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/abc",
            "https://web.push.apple.com/abc",
            "https://1.push.apple.com/abc",
        ] {
            assert!(
                Binding::parse(&token(1), &subscription(endpoint), super::Locale::En).is_some()
            );
        }
        for endpoint in [
            "http://fcm.googleapis.com/path",
            "https://fcm.googleapis.com:444/path",
            "https://user@fcm.googleapis.com/path",
            "https://fcm.googleapis.com/path#fragment",
            "https://fcm.googleapis.com.evil.example/path",
            "https://evilpush.apple.com/path",
            "https://127.0.0.1/path",
            "https://[::1]/path",
            "https://localhost/path",
        ] {
            assert!(
                Binding::parse(&token(1), &subscription(endpoint), super::Locale::En).is_none(),
                "{endpoint}"
            );
        }
        let mut invalid: serde_json::Value =
            serde_json::from_str(&subscription("https://fcm.googleapis.com/path")).unwrap();
        invalid["keys"]["p256dh"] = json!(URL_SAFE_NO_PAD.encode([4; 65]));
        assert!(Binding::parse(&token(1), &invalid.to_string(), super::Locale::En).is_none());
        invalid["keys"]["auth"] = json!("short");
        assert!(Binding::parse("bad", &invalid.to_string(), super::Locale::En).is_none());
        assert!(Binding::parse(&token(1), &"x".repeat(4097), super::Locale::En).is_none());
    }

    #[tokio::test]
    async fn waitlist_registration_is_atomic_with_promotion() {
        let pool = fixture().await;
        let binding = binding(1, "https://fcm.googleapis.com/first");
        let result = signup_with_push(&pool, 1, 2, Some(&binding)).await.unwrap();
        assert_eq!(
            result,
            (SignupOutcome::AddedWaitlist, Some(BindOutcome::Registered))
        );
        cancel_booking_for_slot(&pool, 1, 1).await.unwrap();
        let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM push_jobs WHERE user_id = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, 1);
    }

    #[tokio::test]
    async fn player_signup_does_not_bind_device() {
        let pool = fixture().await;
        sqlx::query("DELETE FROM bookings")
            .execute(&pool)
            .await
            .unwrap();
        let binding = binding(1, "https://fcm.googleapis.com/first");
        let result = signup_with_push(&pool, 1, 2, Some(&binding)).await.unwrap();
        assert_eq!(result, (SignupOutcome::AddedPlayer, None));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn rejected_signups_never_bind_a_device() {
        let pool = fixture().await;
        let binding = binding(1, "https://fcm.googleapis.com/first");
        let duplicate = signup_with_push(&pool, 1, 1, Some(&binding)).await.unwrap();
        assert_eq!(duplicate, (SignupOutcome::AlreadySignedUp, None));
        sqlx::query("UPDATE slots SET max_waitlist = 0 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let full = signup_with_push(&pool, 1, 2, Some(&binding)).await.unwrap();
        assert_eq!(full, (SignupOutcome::Full, None));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn rebinding_invalidates_jobs_but_an_identical_retry_does_not() {
        let pool = fixture().await;
        let first = binding(1, "https://fcm.googleapis.com/first");
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        super::bind(&mut tx, 2, &first).await.unwrap();
        super::bind(&mut tx, 2, &first).await.unwrap();
        let generation: i64 = sqlx::query_scalar("SELECT generation FROM push_devices")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(generation, 0);
        sqlx::query(
            "INSERT INTO push_jobs (kind, device_id, generation, user_id, created_at, next_attempt_at, expires_at)
             VALUES ('test', 1, 0, 2, 0, 0, 9999999999)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let changed = binding(1, "https://fcm.googleapis.com/second");
        super::bind(&mut tx, 2, &changed).await.unwrap();
        let result: (i64, String) = sqlx::query_as(
            "SELECT d.generation, j.status FROM push_devices d JOIN push_jobs j ON j.device_id = d.id",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(result, (1, "skipped".into()));
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn endpoint_collision_saves_booking_without_stealing_device() {
        let pool = fixture().await;
        let first = binding(1, "https://fcm.googleapis.com/first");
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        super::bind(&mut tx, 3, &first).await.unwrap();
        tx.commit().await.unwrap();
        let collision = binding(2, "https://fcm.googleapis.com/first");
        let result = signup_with_push(&pool, 1, 2, Some(&collision))
            .await
            .unwrap();
        assert_eq!(
            result,
            (SignupOutcome::AddedWaitlist, Some(BindOutcome::Unavailable))
        );
        let owner: i64 = sqlx::query_scalar("SELECT user_id FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(owner, 3);
    }

    #[tokio::test]
    async fn waitlist_signup_keeps_alerts_off_and_respects_device_limit() {
        let pool = fixture().await;
        let first = binding(1, "https://fcm.googleapis.com/first");
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        super::bind(&mut tx, 2, &first).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("UPDATE push_devices SET enabled = 0")
            .execute(&pool)
            .await
            .unwrap();
        let result = signup_with_push(&pool, 1, 2, Some(&first)).await.unwrap();
        assert_eq!(
            result,
            (SignupOutcome::AddedWaitlist, Some(BindOutcome::Unavailable))
        );
        let enabled: i64 = sqlx::query_scalar("SELECT enabled FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(enabled, 0);

        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        for n in 2..=6 {
            let other = binding(n, &format!("https://fcm.googleapis.com/{n}"));
            assert_eq!(
                super::bind(&mut tx, 3, &other).await.unwrap(),
                BindOutcome::Registered
            );
        }
        let extra = binding(7, "https://fcm.googleapis.com/extra");
        assert_eq!(
            super::bind(&mut tx, 3, &extra).await.unwrap(),
            BindOutcome::Unavailable
        );
        tx.commit().await.unwrap();
    }

    #[tokio::test]
    async fn total_device_cap_preserves_existing_devices_and_booking() {
        let pool = fixture().await;
        sqlx::raw_sql(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 5000)
             INSERT INTO push_devices (token_hash, enabled, created_at, updated_at)
             SELECT CAST(i AS BLOB), 0, 0, 0 FROM n;",
        )
        .execute(&pool)
        .await
        .unwrap();
        let binding = binding(7, "https://fcm.googleapis.com/extra");
        let result = signup_with_push(&pool, 1, 2, Some(&binding)).await.unwrap();
        assert_eq!(
            result,
            (SignupOutcome::AddedWaitlist, Some(BindOutcome::Unavailable))
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM push_devices")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 5000);
    }

    #[tokio::test]
    async fn binding_database_failure_preserves_waitlist_booking() {
        let pool = fixture().await;
        sqlx::raw_sql(
            "CREATE TRIGGER fail_binding BEFORE INSERT ON push_devices
             BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
        )
        .execute(&pool)
        .await
        .unwrap();
        let binding = binding(1, "https://fcm.googleapis.com/first");
        assert_eq!(
            signup_with_push(&pool, 1, 2, Some(&binding)).await.unwrap(),
            (SignupOutcome::AddedWaitlist, Some(BindOutcome::Unavailable))
        );
        let bookings: i64 = sqlx::query_scalar("SELECT count(*) FROM bookings WHERE user_id = 2")
            .fetch_one(&pool)
            .await
            .unwrap();
        let activity: i64 = sqlx::query_scalar("SELECT count(*) FROM activity_log")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((bookings, activity), (1, 1));
    }
}
