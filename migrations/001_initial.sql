CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    is_admin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE slots (
    id INTEGER PRIMARY KEY,
    datetime TEXT NOT NULL,
    venue TEXT NOT NULL,
    max_players INTEGER NOT NULL DEFAULT 15,
    max_waitlist INTEGER NOT NULL DEFAULT 5
);

CREATE TABLE bookings (
    id INTEGER PRIMARY KEY,
    slot_id INTEGER NOT NULL REFERENCES slots(id),
    user_id INTEGER NOT NULL REFERENCES users(id),
    position INTEGER NOT NULL,
    is_waitlist INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(slot_id, user_id)
);

INSERT INTO users (id, name, is_admin) VALUES
    (1, 'Alex', 1),
    (2, 'Ben', 0),
    (3, 'Chris', 0),
    (4, 'Dani', 0),
    (5, 'Eli', 0),
    (6, 'Farid', 0),
    (7, 'Gio', 0),
    (8, 'Hana', 0),
    (9, 'Ira', 0),
    (10, 'Jamal', 0);

INSERT INTO slots (id, datetime, venue, max_players, max_waitlist) VALUES
    (1, '2026-03-09T20:00:00Z', 'Court A', 15, 5),
    (2, '2026-03-10T20:00:00Z', 'Court B', 15, 5),
    (3, '2026-03-12T20:00:00Z', 'Court C', 15, 5);

INSERT INTO bookings (slot_id, user_id, position, is_waitlist) VALUES
    (1, 1, 1, 0),
    (1, 2, 2, 0),
    (1, 3, 3, 0),
    (1, 4, 4, 0),
    (1, 5, 5, 0),
    (1, 6, 6, 0),
    (1, 7, 1, 1),
    (1, 8, 2, 1),
    (2, 9, 1, 0),
    (2, 10, 2, 0),
    (2, 1, 3, 0),
    (2, 2, 4, 0),
    (2, 3, 5, 0),
    (2, 4, 6, 0),
    (2, 5, 7, 0),
    (2, 6, 8, 0),
    (2, 7, 1, 1),
    (3, 8, 1, 0),
    (3, 9, 2, 0),
    (3, 10, 3, 0),
    (3, 1, 4, 0),
    (3, 2, 5, 0),
    (3, 3, 1, 1),
    (3, 4, 2, 1),
    (3, 5, 3, 1);
