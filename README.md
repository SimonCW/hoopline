# Hoopline

**Hoopline - You're up next!**

Hoopline is a lightweight booking app for recurring pickup basketball slots.
It focuses on fair waitlist handling, clear accountability, and less weekly admin work.

## Run locally

```sh
cargo run
```

Server listens on `0.0.0.0:5050`.

Use a current stable Rust toolchain that supports edition 2024. The member UX changes were checked with Rust 1.99.1.

For a disposable preview with sample data:

```sh
DATABASE_URL=sqlite::memory: cargo run
```

### Member booking behavior

- Upcoming slots appear in weekly groups, with compact cards for phones.
- On the first visit, enter your name before seeing the booking overview. Fuzzy suggestions support partial names and small spelling errors. Select your existing name, or explicitly add a new member.
- Select a suggestion before continuing. The app submits the selected member ID, not the search text. Editing the text clears the selection. Existing names cannot sign in through the new-member form.
- The app remembers your user ID for one year in an `HttpOnly`, `SameSite=Lax` cookie. Returning visitors go straight to the overview. Names and booking details are not stored in the cookie.
- Use **Log out** at the top right if you selected the wrong name or use a shared device. This clears the cookie, not your bookings. Name selection is not authentication.
- Expand **Who's playing?** to see the player list and ordered waitlist.
- A cancellation automatically promotes the first waitlist member. No notification is sent.
- Slots renew every minute within a rolling 14-day local-date window. This is not a Sunday-only release.
- New schedule times use `Europe/Berlin`, including daylight-saving changes. Dates show local times without a time-zone label.
- Existing slots retain their stored instant and bookings after migration. Review legacy slot times separately; they are not silently moved. Generation does not create a duplicate for the same venue and local date.
- Returning to the page refreshes slot state and the selected identity. **Refresh slots** updates them manually.
- Booking and refresh controls stay disabled during shared requests, including page updates. Disabled buttons turn gray. A CSS refresh icon rotates only while the overview updates, without changing the button size. Reduced-motion mode keeps the icon still. Screen readers receive status updates. Errors remain visible.
- Set `COOKIE_SECURE=true` in an HTTPS deployment so the browser sends the identity cookie only over HTTPS. Leave it unset for local HTTP previews.

### Checks

Run `just verify` for formatting, Clippy, and Rust tests. If `just` is not installed:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test --all-targets --all-features
```

Browser tests need Node.js and Chromium:

```sh
npm ci
npx playwright install chromium
just test-ui
```

Without `just`, use `npm run test:ui`. Tests start a disposable server on port 5051. They cover phone and desktop layouts, booking and promotion, keyboard use, automated accessibility checks, slow requests, request failures, and focus refreshes. Rust tests also cover daylight-saving changes and simultaneous signups against a file-backed database.

The app serves its CSS locally. Booking controls need JavaScript and the pinned HTMX script from jsDelivr.

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
