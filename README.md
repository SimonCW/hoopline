# Hoopline

**Hoopline - You're up next!**

Hoopline is a lightweight booking app for recurring pickup basketball slots.
It focuses on fair waitlist handling, clear accountability, and less weekly admin work.

## Run locally

```sh
cargo run
```

To try admin login locally, run `ADMIN_PASSWORD=admin cargo run`. Admin login
is disabled when `ADMIN_PASSWORD` is unset or empty. Use a strong, unique
password in deployment; do not use `admin` outside a local preview.

Server listens on `0.0.0.0:5050`.

Use a current stable Rust toolchain that supports edition 2024. The member UX changes were checked with Rust 1.99.1.

For a disposable preview with sample data:

```sh
just preview
```

This stops this checkout's dev server on port 5050 before starting the preview.
New databases start with no players or bookings. Recurring schedules still
create games. Set `PREVIEW_SAMPLE=1` to add a sample roster to the upcoming
game farthest in the future. This also works
with a file database. The app adds the roster only once,
and only if the database has no players or bookings. It keeps the sample data
in a file database after a restart, even if `PREVIEW_SAMPLE` is later removed.
Run `just stop-dev` to stop that server without starting a new one.
The `just preview` and `just healthcheck` commands use `admin` as the local
admin password unless `ADMIN_PASSWORD` is already set.

For a development preview that rebuilds and restarts when source, templates,
assets, or migrations change, install Bacon (`brew install bacon`) and run:

```sh
just dev
```

Open <http://localhost:5050>. Refresh the browser after Bacon restarts the
server. This preview uses an in-memory database, so changes reset on restart.
Press `q` in Bacon to stop it.

### Member booking behavior

- Games remain visible through their Berlin calendar day. Signups close when a game starts.
- On the first visit, enter your name before seeing the booking overview. Fuzzy suggestions support partial names and small spelling errors. Select your existing name, or explicitly add a new member.
- Select a suggestion before continuing. The app submits the selected member ID, not the search text. Editing the text clears the selection. Existing names cannot sign in through the new-member form.
- The app remembers your user ID for one year in an `HttpOnly`, `SameSite=Lax` cookie. Returning visitors go straight to the overview. Names and booking details are not stored in the cookie.
- Use **Log out** at the top right if you selected the wrong name or use a shared device. This clears the cookie, not your bookings. Name selection is not authentication.
- Expand **Who's playing?** to see the player list and ordered waitlist.
- A cancellation automatically promotes the first waitlist member. No notification is sent.
- New games open every Sunday at 19:00 Berlin time for the week starting eight days later. For example, Sunday 8 November opens 16–22 November. Renewal runs every minute. Admins can change the shared Sunday release time.
- New schedule times use `Europe/Berlin`, including daylight-saving changes. Dates show local times without a time-zone label.
- Existing slots retain their stored instant and bookings after migration. Review legacy slot times separately; they are not silently moved. Generation does not create a duplicate for the same venue and local date.
- Returning to the page refreshes slot state and the selected identity. **Refresh slots** updates them manually.
- Booking actions are tied to the member shown on the page. If another tab changes the selected user, stale actions return an error without changing bookings. Refresh before trying again.
- Booking and refresh controls stay disabled during shared requests, including page updates. Disabled buttons turn gray. A CSS refresh icon rotates only while the overview updates, without changing the button size. Reduced-motion mode keeps the icon still. Screen readers receive status updates. Errors remain visible.
- Set `COOKIE_SECURE=true` in an HTTPS deployment so the browser sends the identity cookie only over HTTPS. Leave it unset for local HTTP previews.

### Admin mode

- Every member can open **Admin UI** and enter the shared admin password. A successful login adds `| Admin` to the selected name and opens admin mode. **Leave Admin** returns to member view and ends the admin session without changing bookings. The session also ends on logout or name change and expires after 12 hours.
- The existing database admin flag does not grant access. Member name selection is still not authenticated. Admin actions require the separate admin session. Five failed password attempts temporarily block admin login for that member for 15 minutes. This limit does not prevent guessing across member names; use a strong password and external rate limiting.
- Admins can add or remove members and reorder the waitlist. Removing a player promotes the first waiting member. Adding a player ahead of the waitlist is an explicit override. These roster controls also work for games that already started.
- **Edit this game** changes one occurrence. When bookings exist, the admin must choose whether to keep or clear them. Clearing removes players and waitlist members without notifications.
- **Edit recurring schedule** changes defaults for newly generated games only. Existing games and bookings stay unchanged. Pausing a schedule does not remove published games. Previously opened games remain available when the release time changes.

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

### Identity checks for booking actions

`src/identity.rs` provides private-field `BookingIdentity` and `AdminIdentity` extractors. Booking handlers require a validated `BookingIdentity`. Admin handlers also require a valid, member-bound admin session through `AdminIdentity`.

HTMX sends the rendered member ID in `X-Hoopline-User`. The extractor checks this numeric ID against the current cookie and database record before the handler runs. Missing or mismatched IDs return HTTP 409 with a refresh instruction. This prevents stale-page mistakes; it is not authentication. Direct API clients must send the same header with booking actions.

## Public access and abuse limits

`robots.txt` asks search engines not to index the site. It does not prevent access.
Anyone can select a player name; this is not authentication and does not prevent impersonation.

The app limits POST requests per connection IP: admin login allows 5 per minute and
20 per hour; player selection allows 10 per minute and 40 per hour; sign-up and
cancellation share a limit of 30 per minute and 200 per hour. A rejected request returns HTTP 429. Limits are held in
memory and reset on restart; each app instance has its own limits.

The app uses the direct peer address and does not trust forwarded IP headers. If
Hostim or another proxy connects to the app, all users behind it may share one
limit. Configure per-client rate limits at a trusted proxy or edge in that case.
Do not expose the app directly if edge protections are required. These limits do
not stop distributed attacks.

## SQLite in production (Hostim)

For production, mount a Hostim volume at `/data` and store SQLite there:

- `DATABASE_URL=sqlite:///data/hoopline.db`
- Set `ADMIN_PASSWORD` to a strong secret and `COOKIE_SECURE=true` for HTTPS. Keep the password out of version control and container images.
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
