"""Seed only the private, migrated scenario database while the app is stopped."""

import sqlite3
import sys

with sqlite3.connect(sys.argv[1]) as db:
    db.execute("PRAGMA foreign_keys = ON")
    # Discard migration sample games in this new database, not in any shared DB.
    db.execute("DELETE FROM bookings")
    db.execute("DELETE FROM slots")
    db.execute("UPDATE slot_schedules SET is_active = 0")
    db.executemany(
        "INSERT INTO users (id, name, is_admin) VALUES (?, ?, ?)",
        [
            (1001, "Scenario Alice", 0),
            (1002, "Scenario Bob", 0),
            (1003, "Scenario Admin", 1),
            *[(1100 + index, f"Seed Player {index:02}", 0) for index in range(1, 15)],
        ],
    )
    profile = sys.argv[2]
    if profile == "last-spot":
        db.execute(
            """INSERT INTO slots
               (id, datetime, venue, max_players, max_waitlist, is_override)
               VALUES (1001, '2030-01-08T18:00:00Z', 'Scenario Court', 15, 5, 1)"""
        )
        db.executemany(
            """INSERT INTO bookings (slot_id, user_id, position, is_waitlist)
               VALUES (1001, ?, ?, 0)""",
            [(1100 + index, index) for index in range(1, 15)],
        )
    elif profile == "release":
        db.execute(
            """INSERT INTO slot_schedules
               (id, weekday, time_local, venue, max_players, max_waitlist, is_active)
               VALUES (1001, 1, '20:00', 'Release Court', 15, 5, 1)"""
        )
    elif profile == "legacy":
        db.executemany(
            "INSERT INTO users (id, name, is_admin) VALUES (?, ?, 0)",
            enumerate(
                ["Alex", "Ben", "Chris", "Dani", "Eli", "Farid", "Gio", "Hana", "Ira", "Jamali"],
                start=1,
            ),
        )
        db.executemany(
            """INSERT INTO slots (id, datetime, venue, max_players, max_waitlist)
               VALUES (?, ?, ?, 15, 5)""",
            [
                (1, "2030-01-07T19:00:00Z", "Luisen"),
                (2, "2030-01-08T19:00:00Z", "Ceci"),
                (3, "2030-01-10T19:00:00Z", "Diesterweg"),
            ],
        )
    else:
        raise ValueError(f"Unknown scenario seed profile: {profile}")
