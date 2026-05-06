# Hoopline

**Hoopline - You're up next!**

Hoopline is a lightweight booking app for recurring pickup basketball slots.
It focuses on fair waitlist handling, clear accountability, and less weekly admin work.

## Run locally

```sh
cargo run
```

Server listens on `0.0.0.0:5050`.

## SQLite in production (Hostim)

For production, mount a Hostim volume at `/data` and store SQLite there:

- `DATABASE_URL=sqlite:///data/hoopline.db`
- Keep local `.db` files out of git (`*.db` is ignored)

This repository's Docker image now defaults `DATABASE_URL` to `/data/hoopline.db`, so with a mounted volume data persists across deploys/restarts.

If `DATABASE_URL` is not set and `/data` is unavailable, the app falls back to local `sqlite://tmp/hoopline.db` for development.

Container startup uses a root entrypoint only to validate/fix `/data` permissions, then immediately drops to `appuser` before running `hoopline`.

## Deploy with Docker

Build and run:

```sh
docker build -t hoopline:latest .
docker run --rm -p 5050:5050 -v "$(pwd)/data:/data" hoopline:latest
```

The included `Dockerfile` is production-ready for persistent SQLite at `/data/hoopline.db`.

## SQLite backup strategy

Use the included script to write timestamped backups and prune old ones:

```sh
./scripts/backup-sqlite.sh /data/hoopline.db /data/backups
```

Recommended cron (every 6 hours):

```cron
0 */6 * * * /app/scripts/backup-sqlite.sh /data/hoopline.db /data/backups
```

Restore example:

```sh
cp /data/backups/hoopline-<timestamp>.db /data/hoopline.db
```
