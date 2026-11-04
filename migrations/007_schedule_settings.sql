ALTER TABLE slot_schedules ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0);
CREATE TABLE schedule_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    sunday_release_time TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)
);
INSERT INTO schedule_settings (id, sunday_release_time) VALUES (1, '19:00');
