CREATE TABLE slot_schedules (
    id INTEGER PRIMARY KEY,
    weekday INTEGER NOT NULL, -- 1=Mon ... 7=Sun
    time_utc TEXT NOT NULL,   -- HH:MM
    venue TEXT NOT NULL,
    max_players INTEGER NOT NULL DEFAULT 15,
    max_waitlist INTEGER NOT NULL DEFAULT 5,
    is_active INTEGER NOT NULL DEFAULT 1,
    UNIQUE (weekday, time_utc, venue)
);

CREATE UNIQUE INDEX slots_datetime_venue_unique ON slots(datetime, venue);

INSERT INTO slot_schedules (weekday, time_utc, venue, max_players, max_waitlist, is_active) VALUES
    (1, '20:00', 'Luisenschule', 15, 5, 1),
    (2, '20:00', 'Ceci', 15, 5, 1),
    (4, '20:00', 'Diesterweg', 15, 5, 1);
