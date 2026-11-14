//! Encrypted delivery over HTTPS to explicitly supported push providers.

use std::{
    future::Future,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::Berlin;
use serde_json::json;
use sqlx::SqlitePool;
use web_push::{
    ContentEncoding, PartialVapidSignatureBuilder, SubscriptionInfo, VapidSignatureBuilder,
    WebPushMessageBuilder,
};

use super::queue::{self, Delivery, ResultKind};

const SEND_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) struct Sender {
    key: PartialVapidSignatureBuilder,
    subject: String,
}

impl Sender {
    /// Load an unpadded base64url P-256 private scalar and verify its public key.
    pub(super) fn new(private: &str, public: &str, subject: &str) -> Result<Self, &'static str> {
        let key = VapidSignatureBuilder::from_base64_no_sub(private)
            .map_err(|_| "invalid VAPID private key")?;
        if URL_SAFE_NO_PAD.encode(key.get_public_key()) != public {
            return Err("VAPID keys do not match");
        }
        let contact = url::Url::parse(subject).map_err(|_| "invalid VAPID subject")?;
        if !matches!(contact.scheme(), "https" | "mailto")
            || subject.len() > 512
            || (contact.scheme() == "https" && contact.host_str().is_none())
            || (contact.scheme() == "mailto" && !contact.path().contains('@'))
        {
            return Err("invalid VAPID subject");
        }
        Ok(Self {
            key,
            subject: subject.into(),
        })
    }

    fn message(
        &self,
        delivery: &Delivery,
        now: i64,
    ) -> Result<web_push::WebPushMessage, &'static str> {
        if !super::valid_endpoint(&delivery.endpoint) {
            return Err("invalid endpoint");
        }
        let (payload, deadline) = payload(delivery)?;
        let deadline = deadline.map_or(delivery.expires_at, |start| start.min(delivery.expires_at));
        let ttl = u32::try_from(deadline.saturating_sub(now)).map_err(|_| "invalid expiry")?;
        if ttl == 0 {
            return Err("expired");
        }
        let subscription =
            SubscriptionInfo::new(&delivery.endpoint, &delivery.p256dh, &delivery.auth);
        let mut signature = self.key.clone().add_sub_info(&subscription);
        signature.add_claim("sub", self.subject.as_str());
        let signature = signature.build().map_err(|_| "invalid signature")?;
        let mut builder = WebPushMessageBuilder::new(&subscription);
        builder.set_ttl(ttl);
        builder.set_vapid_signature(signature);
        builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
        builder.build().map_err(|_| "invalid payload")
    }

    async fn send(&self, delivery: Delivery) -> ResultKind {
        match tokio::time::timeout(SEND_TIMEOUT, self.send_inner(&delivery)).await {
            Ok(result) => result,
            Err(_) => ResultKind::Retry { retry_after: None },
        }
    }

    async fn send_inner(&self, delivery: &Delivery) -> ResultKind {
        let now = crate::clock::now().timestamp();
        let Ok(message) = self.message(delivery, now) else {
            return ResultKind::Invalid;
        };
        let Ok(url) = url::Url::parse(&delivery.endpoint) else {
            return ResultKind::Invalid;
        };
        let Some(host) = url.host_str() else {
            return ResultKind::Invalid;
        };
        let Ok(addresses) = tokio::net::lookup_host((host, 443)).await else {
            return ResultKind::Retry { retry_after: None };
        };
        let addresses: Vec<_> = addresses.collect();
        // Reject the entire DNS answer if any address could reach a private network.
        if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
            return ResultKind::Invalid;
        }
        let Ok(client) = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(SEND_TIMEOUT)
            .resolve_to_addrs(host, &addresses)
            .build()
        else {
            return ResultKind::Configuration;
        };
        let Some(payload) = message.payload else {
            return ResultKind::Invalid;
        };
        let mut request = client
            .post(&delivery.endpoint)
            .header("TTL", message.ttl.to_string())
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream");
        for (name, value) in payload.crypto_headers {
            request = request.header(name, value);
        }
        let Ok(response) = request.body(payload.content).send().await else {
            return ResultKind::Retry { retry_after: None };
        };
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| retry_delay(value, now));
        classify(response.status().as_u16(), retry_after)
    }
}

fn payload(delivery: &Delivery) -> Result<(String, Option<i64>), &'static str> {
    let (title, body, url, tag, deadline) = if let Some(booking) = delivery.booking_id {
        let slot = delivery.slot_id.ok_or("missing slot")?;
        let datetime =
            DateTime::parse_from_rfc3339(delivery.datetime.as_deref().ok_or("missing date")?)
                .map_err(|_| "invalid date")?;
        let venue = delivery.venue.as_deref().ok_or("missing venue")?;
        let local = datetime.with_timezone(&Berlin);
        (
            "You're playing! 🏀",
            format!(
                "You moved off the waitlist for {} at {}. Open Hoopline to check your booking.",
                local.format("%A %d %B at %H:%M"),
                venue
            ),
            format!("/slots#slot-{slot}"),
            Some(format!("promotion:{booking}")),
            Some(datetime.timestamp()),
        )
    } else {
        (
            "Hoopline test alert",
            "Promotion alerts are enabled on this device.".to_owned(),
            "/slots".to_owned(),
            None,
            None,
        )
    };
    let encoded = json!({ "title": title, "body": body, "url": url, "tag": tag }).to_string();
    if encoded.len() > 3000 {
        return Err("payload too large");
    }
    Ok((encoded, deadline))
}

fn retry_delay(value: &str, now: i64) -> Option<i64> {
    value
        .parse::<i64>()
        .ok()
        .filter(|seconds| *seconds >= 0)
        .or_else(|| {
            DateTime::parse_from_rfc2822(value).ok().map(|time| {
                time.with_timezone(&Utc)
                    .timestamp()
                    .saturating_sub(now)
                    .max(0)
            })
        })
}

fn classify(status: u16, retry_after: Option<i64>) -> ResultKind {
    match status {
        200..=299 => ResultKind::Accepted,
        404 | 410 => ResultKind::ExpiredEndpoint,
        401 | 403 => ResultKind::Configuration,
        429 | 500..=599 => ResultKind::Retry { retry_after },
        _ => ResultKind::Invalid,
    }
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => public_ipv6(ip),
    }
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}

fn public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xe000) == 0x2000
        && segments[0] != 0x2002
        && !(segments[0] == 0x2001 && (segments[1] == 0x0db8 || segments[1] < 0x0200))
}

async fn work_once<F, Fut>(pool: &SqlitePool, send: F) -> Result<bool, sqlx::Error>
where
    F: FnOnce(Delivery) -> Fut,
    Fut: Future<Output = ResultKind>,
{
    let now = crate::clock::now().timestamp();
    let Some(claim) = queue::claim(pool, now).await? else {
        return Ok(false);
    };
    let result = if let Some(delivery) = queue::prepare(pool, &claim, now).await? {
        send(delivery).await
    } else {
        ResultKind::Obsolete
    };
    queue::finish(pool, &claim, crate::clock::now().timestamp(), result).await?;
    Ok(true)
}

pub(super) async fn run(pool: SqlitePool, sender: Sender) {
    let mut timer = tokio::time::interval(Duration::from_secs(5));
    let mut last_cleanup = None;
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        if pool.is_closed() {
            break;
        }
        loop {
            match work_once(&pool, |delivery| sender.send(delivery)).await {
                Ok(true) if !pool.is_closed() => {}
                Ok(_) => break,
                Err(_) => {
                    if !pool.is_closed() {
                        eprintln!("push worker database operation failed");
                    }
                    break;
                }
            }
        }
        let now = crate::clock::now().timestamp();
        if last_cleanup.is_none_or(|last| now - last >= 3600) {
            if queue::cleanup(&pool, now).await.is_err() {
                eprintln!("push worker cleanup failed");
            } else {
                last_cleanup = Some(now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use std::net::IpAddr;
    use web_push::VapidSignatureBuilder;

    use super::{Sender, classify, payload, public_ip, retry_delay, work_once};
    use crate::push::queue::{Delivery, ResultKind};

    const PRIVATE: &str = "IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY";
    const PUBLIC_SUBSCRIPTION: &str =
        "BLMbF9ffKBiWQLCKvTHb6LO8Nb6dcUh6TItC455vu2kElga6PQvUmaFyCdykxY2nOSSL3yKgfbmFLRTUaGv4yV8";
    const AUTH: &str = "xS03Fi5ErfTNH_l9WHE9Ig";

    fn sender() -> Sender {
        let public = VapidSignatureBuilder::from_base64_no_sub(PRIVATE)
            .unwrap()
            .get_public_key();
        Sender::new(
            PRIVATE,
            &URL_SAFE_NO_PAD.encode(public),
            "mailto:admin@example.com",
        )
        .unwrap()
    }

    fn delivery() -> Delivery {
        Delivery {
            endpoint: "https://fcm.googleapis.com/test".into(),
            p256dh: PUBLIC_SUBSCRIPTION.into(),
            auth: AUTH.into(),
            expires_at: 4_071_000_000,
            booking_id: Some(42),
            slot_id: Some(123),
            datetime: Some("2099-01-01T19:00:00Z".into()),
            venue: Some("Court".into()),
        }
    }

    #[test]
    fn generates_encrypted_payload_with_bounded_ttl_and_vapid() {
        let sender = sender();
        let delivery = delivery();
        let (plain, start) = payload(&delivery).unwrap();
        let json: serde_json::Value = serde_json::from_str(&plain).unwrap();
        assert_eq!(json["url"], "/slots#slot-123");
        assert_eq!(json["tag"], "promotion:42");
        assert!(json["body"].as_str().unwrap().contains("20:00"));
        let start = start.unwrap();
        let message = sender.message(&delivery, start - 120).unwrap();
        assert_eq!(message.ttl, 120);
        let encrypted = message.payload.unwrap();
        assert_ne!(encrypted.content, plain.as_bytes());
        assert!(
            encrypted
                .crypto_headers
                .iter()
                .any(|(name, value)| *name == "Authorization" && value.starts_with("vapid "))
        );
    }

    #[test]
    fn rejects_unsafe_destinations_and_classifies_provider_status() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "::1",
            "::ffff:127.0.0.1",
            "fe80::1",
            "2001:db8::1",
            "2002:0a00:0001::1",
        ] {
            assert!(!public_ip(ip.parse::<IpAddr>().unwrap()), "{ip}");
        }
        for ip in ["8.8.8.8", "2607:f8b0:400a::1"] {
            assert!(public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(matches!(classify(200, None), ResultKind::Accepted));
        assert!(matches!(classify(410, None), ResultKind::ExpiredEndpoint));
        assert!(matches!(classify(403, None), ResultKind::Configuration));
        assert!(matches!(
            classify(429, Some(60)),
            ResultKind::Retry {
                retry_after: Some(60)
            }
        ));
        assert_eq!(retry_delay("120", 1000), Some(120));
        assert_eq!(
            retry_delay("Thu, 01 Jan 1970 00:18:20 GMT", 1000),
            Some(100)
        );
    }

    #[tokio::test]
    async fn worker_uses_injected_sender_and_records_acceptance() {
        let pool = crate::db::init_pool("sqlite::memory:").await.unwrap();
        let now = crate::clock::now().timestamp();
        sqlx::raw_sql(
            "INSERT INTO users (id, name) VALUES (1, 'Player');
             INSERT INTO slots (id, datetime, venue) VALUES (1, '2099-01-01T19:00:00Z', 'Court');
             INSERT INTO bookings (id, slot_id, user_id, position, is_waitlist) VALUES (1, 1, 1, 1, 0);
             INSERT INTO push_devices (id, token_hash, user_id, endpoint, p256dh, auth, created_at, updated_at)
             VALUES (1, x'01', 1, 'https://fcm.googleapis.com/test', 'key', 'auth', 0, 0);",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO push_jobs (kind, device_id, generation, user_id, booking_id, created_at, next_attempt_at, expires_at)
             VALUES ('waitlist_promoted', 1, 0, 1, 1, ?, ?, ?)",
        )
        .bind(now)
        .bind(now)
        .bind(now + 1800)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            work_once(&pool, |delivery| async move {
                assert_eq!(delivery.booking_id, Some(1));
                ResultKind::Accepted
            })
            .await
            .unwrap()
        );
        let status: String = sqlx::query_scalar("SELECT status FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "accepted");
        assert!(
            !work_once(&pool, |_| async { ResultKind::Accepted })
                .await
                .unwrap()
        );
        sqlx::query("UPDATE bookings SET is_waitlist = 1 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO push_jobs (kind, device_id, generation, user_id, created_at, next_attempt_at, expires_at)
             VALUES ('test', 1, 0, 1, ?, ?, ?)",
        )
        .bind(now)
        .bind(now)
        .bind(now + 1800)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE push_devices SET enabled = 0 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            work_once(&pool, |_| async {
                panic!("obsolete jobs must not be sent");
            })
            .await
            .unwrap()
        );
        let skipped: String =
            sqlx::query_scalar("SELECT status FROM push_jobs WHERE kind = 'test'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(skipped, "skipped");
    }
}
