CREATE TABLE admin_sessions (
    token TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE TABLE admin_login_failures (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, attempted_at INTEGER NOT NULL);
