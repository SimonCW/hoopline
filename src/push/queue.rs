//! Short-lived delivery claims and guarded acknowledgements.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sqlx::{Row, SqlitePool};

const LEASE_SECONDS: i64 = 30;
const MAX_ATTEMPTS: i64 = 6;
const RETRY_SECONDS: [i64; 5] = [10, 30, 120, 300, 900];

pub(super) struct Claim {
    id: i64,
    token: String,
}

pub(super) struct Delivery {
    pub(super) endpoint: String,
    pub(super) p256dh: String,
    pub(super) auth: String,
    pub(super) expires_at: i64,
    pub(super) booking_id: Option<i64>,
    pub(super) slot_id: Option<i64>,
    pub(super) datetime: Option<String>,
    pub(super) venue: Option<String>,
}

pub(super) enum ResultKind {
    Accepted,
    Obsolete,
    Invalid,
    Configuration,
    ExpiredEndpoint,
    Retry { retry_after: Option<i64> },
}

/// Claim one job. The caller must not hold a transaction while sending.
pub(super) async fn claim(pool: &SqlitePool, now: i64) -> Result<Option<Claim>, sqlx::Error> {
    let work: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM push_jobs WHERE
          (status = 'pending' AND (next_attempt_at <= ? OR expires_at <= ? OR attempts >= ?))
          OR (status = 'claimed' AND (lease_until <= ? OR expires_at <= ?)))",
    )
    .bind(now)
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .bind(now)
    .bind(now)
    .fetch_one(pool)
    .await?;
    if !work {
        return Ok(None);
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query(
        "UPDATE push_jobs SET status = 'skipped', completed_at = ?,
         claim_token = NULL, lease_until = NULL
         WHERE id IN (SELECT id FROM push_jobs
           WHERE status IN ('pending', 'claimed') AND expires_at <= ?
           ORDER BY id LIMIT 100)",
    )
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE push_jobs SET status = 'failed', completed_at = ?,
         error_category = 'attempts', claim_token = NULL, lease_until = NULL
         WHERE id IN (SELECT id FROM push_jobs WHERE attempts >= ?
           AND (status = 'pending' OR (status = 'claimed' AND lease_until <= ?))
           ORDER BY id LIMIT 100)",
    )
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM push_jobs
         WHERE expires_at > ? AND attempts < ? AND next_attempt_at <= ?
           AND (status = 'pending' OR (status = 'claimed' AND lease_until <= ?))
         ORDER BY next_attempt_at, id LIMIT 1",
    )
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .bind(now)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = id else {
        tx.commit().await?;
        return Ok(None);
    };
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random)
        .map_err(|_| sqlx::Error::Io(std::io::Error::other("could not create push claim token")))?;
    let token = URL_SAFE_NO_PAD.encode(random);
    sqlx::query(
        "UPDATE push_jobs SET status = 'claimed', claim_token = ?, lease_until = ?,
         attempts = attempts + 1 WHERE id = ?",
    )
    .bind(&token)
    .bind(now + LEASE_SECONDS)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(Claim { id, token }))
}

/// Recheck the current device and booking immediately before sending.
pub(super) async fn prepare(
    pool: &SqlitePool,
    claim: &Claim,
    now: i64,
) -> Result<Option<Delivery>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT d.endpoint, d.p256dh, d.auth, j.expires_at, j.booking_id,
                b.slot_id, s.datetime, s.venue
         FROM push_jobs j
         JOIN push_devices d ON d.id = j.device_id
         LEFT JOIN bookings b ON b.id = j.booking_id
         LEFT JOIN slots s ON s.id = b.slot_id
         WHERE j.id = ? AND j.status = 'claimed' AND j.claim_token = ?
           AND j.lease_until > ? AND j.expires_at > ?
           AND d.enabled = 1 AND d.generation = j.generation AND d.user_id = j.user_id
           AND d.endpoint IS NOT NULL AND d.p256dh IS NOT NULL AND d.auth IS NOT NULL
           AND (j.kind = 'test' OR
                (b.user_id = j.user_id AND b.is_waitlist = 0 AND unixepoch(s.datetime) > ?))",
    )
    .bind(claim.id)
    .bind(&claim.token)
    .bind(now)
    .bind(now)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let endpoint: String = row.try_get("endpoint")?;
    if !super::valid_endpoint(&endpoint) {
        return Ok(None);
    }
    Ok(Some(Delivery {
        endpoint,
        p256dh: row.try_get("p256dh")?,
        auth: row.try_get("auth")?,
        expires_at: row.try_get("expires_at")?,
        booking_id: row.try_get("booking_id")?,
        slot_id: row.try_get("slot_id")?,
        datetime: row.try_get("datetime")?,
        venue: row.try_get("venue")?,
    }))
}

/// Acknowledge only a claim still owned by this worker.
pub(super) async fn finish(
    pool: &SqlitePool,
    claim: &Claim,
    now: i64,
    result: ResultKind,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query(
        "SELECT attempts, expires_at, device_id, generation FROM push_jobs
         WHERE id = ? AND status = 'claimed' AND claim_token = ? AND lease_until > ?",
    )
    .bind(claim.id)
    .bind(&claim.token)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(false);
    };
    let attempts: i64 = row.try_get("attempts")?;
    let expires_at: i64 = row.try_get("expires_at")?;
    let (status, category, next, complete) = match result {
        ResultKind::Accepted => ("accepted", None, now, Some(now)),
        ResultKind::Obsolete => ("skipped", Some("obsolete"), now, Some(now)),
        ResultKind::Invalid => ("failed", Some("invalid"), now, Some(now)),
        ResultKind::Configuration => ("failed", Some("configuration"), now, Some(now)),
        ResultKind::ExpiredEndpoint => {
            let device: i64 = row.try_get("device_id")?;
            let generation: i64 = row.try_get("generation")?;
            let changed = sqlx::query(
                "UPDATE push_devices SET endpoint = NULL, p256dh = NULL, auth = NULL,
                 generation = generation + 1, updated_at = ?
                 WHERE id = ? AND generation = ?",
            )
            .bind(now)
            .bind(device)
            .bind(generation)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if changed == 1 {
                sqlx::query(
                    "UPDATE push_jobs SET status = 'skipped', completed_at = ?,
                     error_category = 'expired', claim_token = NULL, lease_until = NULL
                     WHERE device_id = ? AND id != ? AND status IN ('pending', 'claimed')",
                )
                .bind(now)
                .bind(device)
                .bind(claim.id)
                .execute(&mut *tx)
                .await?;
            }
            ("failed", Some("expired"), now, Some(now))
        }
        ResultKind::Retry { retry_after } => {
            let index = usize::try_from((attempts - 1).clamp(0, 4)).unwrap_or(4);
            let delay = RETRY_SECONDS[index].max(retry_after.unwrap_or(0));
            let next = now.saturating_add(delay);
            if attempts >= MAX_ATTEMPTS || next >= expires_at {
                ("failed", Some("retry"), now, Some(now))
            } else {
                ("pending", Some("retry"), next, None)
            }
        }
    };
    sqlx::query(
        "UPDATE push_jobs SET status = ?, error_category = ?, next_attempt_at = ?,
         completed_at = ?, claim_token = NULL, lease_until = NULL
         WHERE id = ? AND status = 'claimed' AND claim_token = ?",
    )
    .bind(status)
    .bind(category)
    .bind(next)
    .bind(complete)
    .bind(claim.id)
    .bind(&claim.token)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub(super) async fn cleanup(pool: &SqlitePool, now: i64) -> Result<u64, sqlx::Error> {
    let deleted = sqlx::query(
        "DELETE FROM push_jobs WHERE id IN
         (SELECT id FROM push_jobs WHERE completed_at <= ? ORDER BY id LIMIT 100)",
    )
    .bind(now - 7 * 24 * 60 * 60)
    .execute(pool)
    .await?
    .rows_affected();
    sqlx::query(
        "DELETE FROM push_devices WHERE id IN
         (SELECT id FROM push_devices WHERE updated_at < ? ORDER BY id LIMIT 100)",
    )
    .bind(now - 366 * 24 * 60 * 60)
    .execute(pool)
    .await?;
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::{ResultKind, claim, cleanup, finish, prepare};

    async fn fixture() -> sqlx::SqlitePool {
        let pool = crate::db::init_pool("sqlite::memory:").await.unwrap();
        seed(&pool).await;
        pool
    }

    async fn seed(pool: &sqlx::SqlitePool) {
        sqlx::raw_sql(
            "INSERT INTO users (id, name) VALUES (1, 'Player');
             INSERT INTO slots (id, datetime, venue) VALUES (1, '2099-01-01T19:00:00Z', 'Court');
             INSERT INTO bookings (id, slot_id, user_id, position, is_waitlist) VALUES (1, 1, 1, 1, 0);
             INSERT INTO push_devices (id, token_hash, user_id, endpoint, p256dh, auth, created_at, updated_at)
             VALUES (1, x'01', 1, 'https://fcm.googleapis.com/test', 'key', 'auth', 0, 0);
             INSERT INTO push_jobs (kind, device_id, generation, user_id, booking_id, created_at, next_attempt_at, expires_at)
             VALUES ('waitlist_promoted', 1, 0, 1, 1, 1000, 1000, 2800);",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn concurrent_file_database_workers_cannot_claim_same_job() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("queue.db").display());
        let first = crate::db::init_pool(&url).await.unwrap();
        seed(&first).await;
        let second = crate::db::init_pool(&url).await.unwrap();
        let (a, b) = tokio::join!(claim(&first, 1000), claim(&second, 1000));
        assert_ne!(a.unwrap().is_some(), b.unwrap().is_some());
    }

    #[tokio::test]
    async fn lease_recovery_guards_acknowledgement() {
        let pool = fixture().await;
        let first = claim(&pool, 1000).await.unwrap().unwrap();
        assert!(claim(&pool, 1029).await.unwrap().is_none());
        let second = claim(&pool, 1030).await.unwrap().unwrap();
        assert!(
            !finish(&pool, &first, 1031, ResultKind::Accepted)
                .await
                .unwrap()
        );
        assert!(
            finish(&pool, &second, 1031, ResultKind::Accepted)
                .await
                .unwrap()
        );
        let status: String = sqlx::query_scalar("SELECT status FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "accepted");
    }

    #[tokio::test]
    async fn prepare_rechecks_booking_device_and_live_game_details() {
        let pool = fixture().await;
        let claimed = claim(&pool, 1000).await.unwrap().unwrap();
        sqlx::query("UPDATE slots SET venue = 'New court' WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        let delivery = prepare(&pool, &claimed, 1001).await.unwrap().unwrap();
        assert_eq!(delivery.venue.as_deref(), Some("New court"));
        sqlx::query("UPDATE bookings SET is_waitlist = 1 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        assert!(prepare(&pool, &claimed, 1001).await.unwrap().is_none());
        sqlx::query("UPDATE bookings SET is_waitlist = 0 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE push_devices SET generation = 1 WHERE id = 1")
            .execute(&pool)
            .await
            .unwrap();
        assert!(prepare(&pool, &claimed, 1001).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn retries_respect_backoff_retry_after_and_expiry() {
        let pool = fixture().await;
        let first = claim(&pool, 1000).await.unwrap().unwrap();
        assert!(
            finish(
                &pool,
                &first,
                1000,
                ResultKind::Retry {
                    retry_after: Some(45)
                }
            )
            .await
            .unwrap()
        );
        assert!(claim(&pool, 1044).await.unwrap().is_none());
        let second = claim(&pool, 1045).await.unwrap().unwrap();
        finish(
            &pool,
            &second,
            1045,
            ResultKind::Retry {
                retry_after: Some(2000),
            },
        )
        .await
        .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "failed");
    }

    #[tokio::test]
    async fn expired_endpoint_clears_credentials_and_other_pending_jobs() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO push_jobs (kind, device_id, generation, user_id, created_at, next_attempt_at, expires_at)
             VALUES ('test', 1, 0, 1, 1000, 1000, 2800)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let claimed = claim(&pool, 1000).await.unwrap().unwrap();
        finish(&pool, &claimed, 1001, ResultKind::ExpiredEndpoint)
            .await
            .unwrap();
        let device: (Option<String>, i64) =
            sqlx::query_as("SELECT endpoint, generation FROM push_devices")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(device, (None, 1));
        let jobs: Vec<String> = sqlx::query_scalar("SELECT status FROM push_jobs ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, ["failed", "skipped"]);
    }

    #[tokio::test]
    async fn expired_jobs_are_not_claimed_and_cleanup_is_bounded() {
        let pool = fixture().await;
        assert!(claim(&pool, 2800).await.unwrap().is_none());
        let status: String = sqlx::query_scalar("SELECT status FROM push_jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "skipped");
        assert_eq!(cleanup(&pool, 2800 + 7 * 24 * 60 * 60).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn cleanup_keeps_recent_off_choices_and_removes_old_records() {
        let pool = fixture().await;
        let now = 400 * 24 * 60 * 60;
        sqlx::query(
            "INSERT INTO push_devices (token_hash, enabled, created_at, updated_at)
             VALUES (x'02', 0, ?, ?), (x'03', 0, ?, ?)",
        )
        .bind(now - 365 * 24 * 60 * 60)
        .bind(now - 365 * 24 * 60 * 60)
        .bind(now - 367 * 24 * 60 * 60)
        .bind(now - 367 * 24 * 60 * 60)
        .execute(&pool)
        .await
        .unwrap();
        cleanup(&pool, now).await.unwrap();
        let hashes: Vec<Vec<u8>> = sqlx::query_scalar("SELECT token_hash FROM push_devices")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(hashes, vec![vec![2]]);
    }
}
