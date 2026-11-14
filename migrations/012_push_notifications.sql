-- A device token proves ownership of one browser installation, not a member.
CREATE TABLE push_devices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token_hash BLOB NOT NULL UNIQUE,
    user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    endpoint TEXT UNIQUE,
    p256dh TEXT,
    auth TEXT,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    generation INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK ((endpoint IS NULL AND p256dh IS NULL AND auth IS NULL)
        OR (endpoint IS NOT NULL AND p256dh IS NOT NULL AND auth IS NOT NULL))
);

CREATE INDEX push_devices_user ON push_devices(user_id);

CREATE TABLE push_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('waitlist_promoted', 'test')),
    device_id INTEGER NOT NULL REFERENCES push_devices(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    booking_id INTEGER REFERENCES bookings(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    next_attempt_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    completed_at INTEGER,
    attempts INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'claimed', 'accepted', 'skipped', 'failed')),
    error_category TEXT,
    claim_token TEXT,
    lease_until INTEGER,
    CHECK ((kind = 'waitlist_promoted' AND booking_id IS NOT NULL)
        OR (kind = 'test' AND booking_id IS NULL))
);

CREATE UNIQUE INDEX push_jobs_promotion ON push_jobs(booking_id, device_id, kind)
    WHERE kind = 'waitlist_promoted';
CREATE INDEX push_jobs_due ON push_jobs(status, next_attempt_at);
