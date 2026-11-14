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
- A member cancellation automatically promotes the first waitlist member. When
  configured, registered devices can receive a best-effort promotion alert.
  Admin actions do not send alerts. Check the roster for the current booking.
- New games open every Sunday at 19:00 Berlin time for the week starting eight days later. For example, Sunday 8 November opens 16–22 November. Renewal runs every minute. Admins can change the shared Sunday release time.
- New schedule times use `Europe/Berlin`, including daylight-saving changes. Dates show local times without a time-zone label.
- Existing slots retain their stored instant and bookings after migration. Review legacy slot times separately; they are not silently moved. Generation does not create a duplicate for the same venue and local date.
- Returning to the page refreshes slot state and the selected identity. **Refresh slots** updates them manually.
- Booking actions are tied to the member shown on the page. If another tab changes the selected user, stale actions return an error without changing bookings. Refresh before trying again.
- Booking and refresh controls stay disabled during shared requests, including page updates. Disabled buttons turn gray. A CSS refresh icon rotates only while the overview updates, without changing the button size. Reduced-motion mode keeps the icon still. Screen readers receive status updates. Errors remain visible.
- Set `COOKIE_SECURE=true` in an HTTPS deployment so the browser sends the identity cookie only over HTTPS. Leave it unset for local HTTP previews.

### Promotion alerts and installation

Hoopline can be installed as an online-only web app. On iPhone or iPad, use
**Share → Add to Home Screen**, select **Open as Web App** if offered, and open
Hoopline from the Home Screen. Select your name again there; browser storage
may not transfer. Installation is optional and does not grant notification
permission. Booking needs an internet connection.

When alerts are configured, use **Enable promotion alerts** on the games page.
If permission is already granted, joining a waitlist also registers this device.
Use **Turn off alerts on this device** to prevent automatic registration until
you enable alerts again. Each device has its own setting. Delivery is best effort:
the push provider may accept an alert without the player seeing it. A push
cannot be recalled after the provider accepts it. Check the roster for changes.

Push is disabled by default. To enable it, set:

- `PUSH_ENABLED=true`
- `APP_ORIGIN` to the canonical HTTPS origin, such as `https://example.com`.
  On Hostim, if this is unset, Hoopline uses the HTTPS origin of Hostim's
  `BUILTIN_DOMAIN`. Set `APP_ORIGIN` explicitly when using a custom domain.
- `VAPID_PRIVATE_KEY` to a stable P-256 private scalar as unpadded base64url.
- `VAPID_PUBLIC_KEY` to the matching uncompressed public key as unpadded base64url.
- `VAPID_SUBJECT` to an operator contact, such as `mailto:admin@example.com`.

The app rejects invalid settings at startup. Keep the private key secret, stable
across restarts, and backed up. Key rotation can require device resubscription.
Set `COOKIE_SECURE=true` for the member cookie as well. The push device cookie is
always secure when push is enabled. The host needs outbound HTTPS access to
Apple, Mozilla, and Google push services and persistent SQLite storage. Failed
delivery is retried until expiry, at most six times. Alerts expire at the earlier
of the game start or 30 minutes after promotion.

The database accepts up to five active devices per member and 5,000 stored
devices in total. It removes device records after 366 days without an update,
in batches. An explicit off choice is retained for at least the one-year device
cookie lifetime. Native builds need OpenSSL development headers and `pkg-config`.

Name selection is not authentication. A visitor can select another member's name
and register their own device for that member. Device ownership prevents changes
to another device, but does not prevent impersonation.

### Admin mode

- Every member can open **Admin UI** and enter the shared admin password. A successful login adds `| Admin` to the selected name and opens admin mode. **Leave Admin** returns to member view and ends the admin session without changing bookings. The session also ends on logout or name change and expires after 12 hours.
- The existing database admin flag does not grant access. Member name selection is still not authenticated. Admin actions require the separate admin session. Five failed password attempts temporarily block admin login for that member for 15 minutes. This limit does not prevent guessing across member names; use a strong password and external rate limiting.
- Admins can add or remove members and reorder the waitlist. Removing a player promotes the first waiting member. Adding a player ahead of the waitlist is an explicit override. These roster controls also work for games that already started.
- **Edit this game** changes one occurrence. When bookings exist, the admin must choose whether to keep or clear them. Clearing removes players and waitlist members without notifications.
- **Edit recurring schedule** changes defaults for newly generated games only. Existing games and bookings stay unchanged. Pausing a schedule does not remove published games. Previously opened games remain available when the release time changes.

### Activity log

The `activity_log` table uses the same SQLite database as bookings. With 75
members and about 30 requests per minute at peak, a second database adds no
useful capacity. One transaction writes the booking change and its history.
If the log write fails, the booking change also fails. Existing backups include
the log.

The app records successful member booking changes:

- `booking.signed_up`: a member joins the player list or waitlist.
- `booking.cancelled`: a member leaves either list.
- `booking.promoted`: a cancellation moves the first waiter to the player list.

Admin operations, including their automatic promotions, do not create entries.
Selecting a name, logging out, page views, rejected requests, and preview data
do not create entries. There is no history before this migration. This is
activity history, not a complete record from which to rebuild the roster.

Each row has an increasing ID, an action kind, an actor, and a subject.
The actor is the member who caused the action. The subject is the member whose
booking changed. For automatic promotion, these are different members.
Names, game time, and venue are snapshots. Later edits do not change them.
Deleting a member or game clears its foreign-key references, but keeps the
snapshots. This prevents new records with reused IDs from inheriting old history.
Deleted members and games cannot be grouped reliably across historical entries.
Their names, times, and venues are not unique keys. Analysis by member or game
must exclude cleared references. The log does not establish attendance or games
played; a future ranking needs a separate, clearly defined participation measure.
Member selection is not authentication. These entries identify the selected
member, not a verified person.

`occurred_at` and `slot_starts_at` use UTC RFC 3339 timestamps.
`occurred_local_date` and `slot_local_date` use `Europe/Berlin`, including
daylight-saving changes. The game fields are nullable for future actions that
do not concern a game. Indexes support searches by member, game, and action kind.

`details` is a JSON object. The separate `schema_version` column identifies its
format; the current version is 1. For booking actions, `details` has:

- `booking_id` and `booked_at`: the source booking ID and its creation time.
  The creation time uses SQLite's UTC `YYYY-MM-DD HH:MM:SS` format.
  Booking IDs use SQLite `AUTOINCREMENT` and are not reused by the app after
  deletion. The same ID links all events for one booking lifecycle, including
  bookings added by admins. A rebooking gets a new ID, even in the same second.
  Activity IDs identify individual events. The migration preserves existing
  bookings and their IDs, but cannot recover events from before the migration.
- `list`: `players` or `waitlist`.
- `position`: the position when the action was recorded.
- `caused_by_activity_id`: the cancellation event ID for an automatic promotion.

Signup snapshots show the new booking. Cancellation and promotion snapshots show
the booking before the change. A promotion always changes `waitlist` to `players`.
New action kinds can add their own JSON fields without a new table. Change
`schema_version` if the meaning or structure of existing details changes.
The app only appends entries. It has no log UI or public log endpoint.
Keep database access restricted: the log contains member names and booking history.

For example, this query counts player cancellations on game day within two hours
before the start. It excludes waitlist departures and cancellations after start:

```sql
SELECT subject_user_id, COUNT(*) AS late_cancellations
FROM activity_log
WHERE kind = 'booking.cancelled'
  AND subject_user_id IS NOT NULL
  AND json_extract(details, '$.list') = 'players'
  AND occurred_local_date = slot_local_date
  AND julianday(occurred_at) BETWEEN julianday(slot_starts_at, '-2 hours')
                               AND julianday(slot_starts_at)
GROUP BY subject_user_id;
```

For a fair ranking, also include members with zero cancellations. Define a
minimum number of games and a reporting period. Admin removals are not member
cancellations. Keep those rules in the future feature, not in the stored events.

### Checks

Install Rust, Node.js, Python 3, Just, and Chromium. Set up browser dependencies:

```sh
npm ci
npx playwright install chromium
just test-all
```

`just test-all` runs the same complete suite as CI. See
[local testing](docs/testing.md) for focused commands, isolation, and diagnostics.

### Identity checks for booking actions

`src/identity.rs` provides private-field `BookingIdentity` and `AdminIdentity` extractors. Booking handlers require a validated `BookingIdentity`. Admin handlers also require a valid, member-bound admin session through `AdminIdentity`.

HTMX sends the rendered member ID in `X-Hoopline-User`. The extractor checks this numeric ID against the current cookie and database record before the handler runs. Missing or mismatched IDs return HTTP 409 with a refresh instruction. This prevents stale-page mistakes; it is not authentication. Direct API clients must send the same header with booking actions.

## Public access and abuse limits

`robots.txt` asks search engines not to index the site. It does not prevent access.
Anyone can select a player name; this is not authentication and does not prevent impersonation.

The app limits POST requests per connection IP: admin login allows 5 per minute and
20 per hour; player selection allows 5 per minute and 10 per hour; sign-up and
cancellation share a limit of 10 per minute and 20 per hour. A rejected request returns HTTP 429. Limits are held in
memory and reset on restart; each app instance has its own limits.

The app uses the rightmost address in `X-Forwarded-For`, falling back to the
direct peer address if it is missing or invalid. This assumes the last proxy
adds or replaces that address with the actual client IP. Confirm this behavior
with Hostim, and prevent direct access to the app. Otherwise clients can forge
the header and bypass the limits. Additional proxies may cause users to share a
limit. These limits do not stop distributed attacks.

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
