# Hoopline

**Hoopline - You're up next!**

Hoopline is a lightweight booking app for recurring pickup basketball slots.
It focuses on fair waitlist handling, clear accountability, and less weekly admin work.

## SQLite in production (Hostim)

For production, mount a Hostim volume at `/data` and store SQLite there:

- `DATABASE_URL=sqlite:///data/hoopline.db`
- Keep local `.db` files out of git (`*.db` is ignored)

This repository's Docker image now defaults `DATABASE_URL` to `/data/hoopline.db`, so with a mounted volume data persists across deploys/restarts.

If `DATABASE_URL` is not set and `/data` is unavailable, the app falls back to local `sqlite://tmp/hoopline.db` for development.

Container startup uses a root entrypoint only to validate/fix `/data` permissions, then immediately drops to `appuser` before running `hoopline`.
