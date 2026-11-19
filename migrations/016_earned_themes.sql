ALTER TABLE slots ADD COLUMN theme_progress_processed INTEGER NOT NULL DEFAULT 0;
CREATE INDEX slots_pending_theme_progress ON slots(julianday(datetime))
    WHERE theme_progress_processed = 0;

CREATE TABLE theme_progress (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    qualifying_games INTEGER NOT NULL CHECK (qualifying_games > 0)
);

CREATE TABLE theme_unlocks (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    theme TEXT NOT NULL,
    activity_id INTEGER NOT NULL REFERENCES activity_log(id),
    PRIMARY KEY (user_id, theme)
);
