-- Keep booking IDs as permanent lifecycle keys, including admin-created bookings.
-- Rebuild the table because SQLite cannot add AUTOINCREMENT with ALTER COLUMN.
CREATE TABLE bookings_with_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slot_id INTEGER NOT NULL REFERENCES slots(id),
    user_id INTEGER NOT NULL REFERENCES users(id),
    position INTEGER NOT NULL,
    is_waitlist INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(slot_id, user_id)
);

INSERT INTO bookings_with_history (id, slot_id, user_id, position, is_waitlist, created_at)
SELECT id, slot_id, user_id, position, is_waitlist, created_at FROM bookings;
DROP TABLE bookings;
ALTER TABLE bookings_with_history RENAME TO bookings;

-- Keep history when a member or game is deleted. Snapshots remain unchanged.
CREATE TABLE activity_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at TEXT NOT NULL,
    occurred_local_date TEXT NOT NULL,
    kind TEXT NOT NULL,
    actor_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    actor_name TEXT NOT NULL,
    subject_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    subject_name TEXT NOT NULL,
    slot_id INTEGER REFERENCES slots(id) ON DELETE SET NULL,
    slot_starts_at TEXT,
    slot_local_date TEXT,
    venue TEXT,
    schema_version INTEGER NOT NULL DEFAULT 1,
    details TEXT NOT NULL CHECK (json_valid(details) AND json_type(details) = 'object')
);

CREATE INDEX activity_log_subject_time ON activity_log(subject_user_id, occurred_at, id);
CREATE INDEX activity_log_slot_time ON activity_log(slot_id, occurred_at, id);
CREATE INDEX activity_log_kind_time ON activity_log(kind, occurred_at, id);
