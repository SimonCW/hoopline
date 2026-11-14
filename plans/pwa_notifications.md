# PWA and Waitlist Promotion Notifications

Status: Implemented. Automated checks passed; physical-device and container checks remain.
Date: 2026-11-13.
Baseline: `main` at `66d7278`. PR #11 and PR #12 are merged in this baseline.

## 1. Result and Scope

Make Hoopline installable as a small, online-only PWA. Add Web Push alerts when
a **member cancellation** automatically promotes another member from a waitlist.
Keep the current account-free booking flow.

The user approved these rules:

1. Do not notify for admin-triggered promotions.
2. Joining a waitlist automatically enables the device's promotion alerts when
   browser permission is already granted. Do not require a second Subscribe click.
3. Keep automatic promotion. Do not require the promoted member to accept a place.
4. Add a minimal PWA. Do not build a native app or replace the existing frontend.

Non-goals:

- Accounts, passwords, email verification, or changes to member authentication.
- Email, SMS, WhatsApp, reminder, release-time, or game-change notifications.
- Admin notifications, acceptance offers, or attendance tracking.
- Per-game notification preferences, offline booking, background synchronization,
  offline roster caches, app stores, or a hosted notification platform.

Notifications remain best-effort. Hoopline must never claim that a player saw an
alert. The saved roster remains the source of truth.

## 2. Product Rules

### What triggers an alert

| Action | Send a promotion alert? |
| --- | --- |
| Member cancels a player booking and the first waiter moves into Players | Yes |
| Member cancels a waitlist booking | No |
| Member cancels when nobody waits | No |
| Admin removes a player and that removal promotes a waiter | No |
| Admin explicitly promotes, adds, removes, or reorders a member | No |
| Game edit clears bookings | No |
| An admin uses the normal member cancellation route for their own booking | Yes |
| Rejected action, duplicate retry, preview seed, or historical activity | No |

Determine the cause from the mutation entry point, not the actor's admin status.
Do not change roster rules or activity history to implement notifications.

### What “automatically subscribe” means

There are three separate states:

1. Browser permission: `default`, `granted`, or `denied`.
2. Browser push subscription: the device's endpoint and encryption keys.
3. Hoopline registration: the endpoint is bound to the selected member.

Permission alone is not an active Hoopline registration.

- On a successful waitlist signup, register this device for the selected member
  automatically if permission is granted and push setup succeeds.
- One registration covers that member's current and future waitlist promotions.
  No separate watch table or per-game Subscribe button is needed.
- Only devices registered for that member receive alerts. Joining on one phone
  cannot grant permission on another phone.
- Existing registered devices continue to work when the member books elsewhere.
- A fresh device with granted permission must also work. Do not assume that it
  already has a push subscription or a matching server registration.
- Do not enroll new devices on page load, name selection, roster viewing, or an
  admin action. Page load may inspect capability and existing device state.
- Reconcile an existing device registration after permission revocation or
  subscription expiry. Do not claim that it is still active.
- If the server marks an endpoint expired, replace the stale browser subscription
  on the next Enable or granted-permission signup gesture. Do not repeatedly
  register the same known-expired endpoint.
- An explicit **Turn off alerts on this device** choice overrides automatic
  enrollment. Keep that choice until the player explicitly selects Enable again.
  Browser permission can remain granted after the player turns alerts off.

The server decides whether a signup joins Players or Waitlist. Do not decide
enrollment from the button label. A stale “Sign Up” can result in a waitlist
booking. A stale “Join Waitlist” can result in a player booking.

### Permission and installation

- Never request notification permission on page load or in a delayed network
  callback.
- If permission is `default`, save the waitlist booking normally. Show an
  **Enable promotion alerts** action beside the saved waitlist status.
- That explicit action requests permission directly from the user's gesture.
  On grant, create the browser subscription and register it automatically.
  Do not add another Subscribe step.
- Enabling also covers existing waitlist bookings for this member. Refresh the
  roster afterward. Do not replay promotions that happened before registration.
- If permission is denied, keep booking available. Explain how to change browser
  settings. Do not repeatedly show the browser prompt.
- On iPhone/iPad, explain **Add to Home Screen → open Hoopline there → enable
  alerts**. Include the “Open as Web App” option where the OS shows it.
- The installed app can have separate storage from the browser tab. Let the
  player select their name again. Do not promise that Safari cookies transfer.
- An unsupported browser or failed installation must not prevent booking.

### Feedback

Use concise status text:

- Registered: “Promotion alerts are on for Alex on this device.”
- Waiting without alerts: “You are on the waitlist. Enable alerts to hear when
  you move up.”
- Setup failed: “Booking saved. Alerts could not be enabled. Try again.”
- Explicitly off: “Alerts are off on this device.”
- Unsupported: “This browser cannot receive alerts. Check your booking here.”

Only show “on” after the server confirms registration. Announce status changes
accessibly. Do not replace a successful booking message with a booking error
when only push setup failed. No disruptive banners or automatic test alerts.
Provide **Send test alert** and **Turn off alerts on this device** controls.

## 3. Architecture

Keep Rust, Axum, Askama, HTMX, Tokio, and the existing SQLite database.

```text
Waitlist signup + valid optional subscription
  → save booking and device binding in one SQLite transaction

Member player cancellation
  → promote first waiter + save delivery jobs in one SQLite transaction
  → commit
  → background worker claims jobs
  → encrypted Web Push through the browser's push provider
  → service worker shows notification
  → tap opens the current roster
```

Use a maintained Rust Web Push library for VAPID and `aes128gcm` encryption.
`web-push` is a candidate. Check its current release, TLS/native dependencies,
timeouts, redirects, and Docker compatibility before choosing features. Do not
implement the cryptography. Keep the sender small; do not add a generic messaging
framework or a second queue service.

### Relevant current code

| Location | Work |
| --- | --- |
| `src/lib.rs` | Routes, signup form handling, identity changes, response feedback, worker startup |
| `src/db.rs` | Transactional signup, cancellation, and `promote_first_waiter` |
| `src/activity.rs` | Preserve current history behavior; do not poll it for delivery |
| `src/identity.rs` | Reuse `BookingIdentity` and stale displayed-member protection |
| `src/rate_limit.rs` | Add push setup and test-send limits |
| `src/clock.rs` | Reuse the existing testable application clock |
| `templates/base.html` | Manifest, icons, theme metadata, local PWA script |
| `templates/slot_card.html` | Waitlist alert status and conditional enable action |
| `assets/app.js` | Integrate asynchronous preparation with the shared request lock |
| `tests/scenarios/fixtures.js` | Isolated processes; service workers are blocked by default |
| `docs/testing.md`, `README.md` | Setup, browser limits, checks, and deployment instructions |

New code can use `src/push.rs`, small private submodules if needed,
`assets/pwa.js`, and `assets/sw.js`. Follow surrounding conventions.

## 4. Minimal PWA

Serve these resources locally:

- `/manifest.webmanifest`, with `application/manifest+json`.
- `/sw.js`, with a JavaScript content type and scope `/`.
- App icons, including 192×192, 512×512, an appropriate maskable icon, and an
  Apple touch icon. Use a simple Hoopline mark, not a wallpaper or player photo.

Manifest settings:

- Stable `id: "/"`, `start_url: "/"`, and `scope: "/"`.
- `name: "Hoopline"`, `short_name: "Hoopline"`, `display: "standalone"`.
- Suitable background and theme colors. Add matching head metadata.

Register the worker early so it can be ready when a player taps a booking or
Enable button. Use feature detection. Do not wait indefinitely for registration.

The worker handles `push` and `notificationclick`. **Do not add a fetch handler,
Cache Storage roster caching, offline writes, or background sync.** The PWA is
online-only. Explain loss of connection and keep normal booking error handling.
Never display cached booking success or availability as current.

Do not force reload open booking pages for an update. Use the normal worker
lifecycle. Keep payloads backward-compatible with the previous worker.
Serve the manifest and worker with revalidation or `no-store`, not immutable
caching. Account for the router's current global `Cache-Control: no-store`.
Keep dynamic pages and identity responses non-cacheable.

Installability must not require notification permission. Booking must not require
installation. Provide a small, dismissible install help section. Use an install
prompt only when the browser offers one; provide manual iOS instructions.

## 5. Device Ownership and Storage

Use a new additive migration, currently `012_push_notifications.sql`. Recheck
the next migration number before implementation. Do not edit applied migrations.

Use two tables. Group related configuration in strong Rust types.

### `push_devices`

Store:

- An internal, non-reused ID.
- A hash of a cryptographically random 256-bit device ownership token.
- Nullable member reference, with `ON DELETE SET NULL`.
- Unique endpoint, `p256dh`, and `auth` keys when a subscription exists.
- An explicit enabled/disabled preference.
- A binding generation, incremented when the member, endpoint, or preference changes.
- Created and updated timestamps.

Use one member binding per browser installation. Allow several installations
per member. Keep a small documented cap, such as five registered devices per
member. Reject extra registrations visibly; do not evict another device.

Keep the raw ownership token in a separate `HttpOnly`, `SameSite=Lax`, path `/`
cookie. Use `Secure` in HTTPS deployments and a one-year lifetime, consistent
with the member cookie. Issue it during the member overview response, before
optional signup preparation. Missing cookies must still permit ordinary booking.
The cookie is not member authentication.

An endpoint collision must not transfer another device's registration. Return a
setup error when the ownership token does not match. Do not expose tokens,
endpoints, or keys in rendered HTML, public listings, logs, or errors.

On logout or name selection, detach this device from the previous member,
increment its generation, and cancel its pending jobs in the same transaction
as the identity-related database changes. Preserve its explicit off preference.
Do not silently move an old registration to the new name. A later waitlist join
or explicit Enable action can bind this device to the new selected member.
Do not unsubscribe other devices belonging to the old member.

### `push_jobs`

Store one delivery per promoted booking and registered device:

- Non-reused job ID and kind, initially only `waitlist_promoted` and `test`.
- Device ID, expected binding generation, and expected recipient member.
- Booking ID for promotion jobs. Use PR #11's non-reused booking lifecycle ID.
- Created, next-attempt, expiry, and completion timestamps.
- Attempt count, status, short error category, and claim lease information.

Use foreign keys and appropriate cascading cleanup. Member/game/booking deletion
must not leave deliverable promotion jobs for reused IDs.
Use a unique constraint on `(booking_id, device_id, kind)` for promotion jobs.
Test alerts need their own IDs and must not collide with real promotions.

Create jobs only for devices registered and enabled at promotion time. Do not
send historical promotions to a device registered later.
Do not add delivery status fields to the immutable activity log.

## 6. Atomic Waitlist Enrollment

Avoid a signup-then-register race. A player can be promoted immediately after
joining the waitlist.

1. Add an optional serialized push subscription field to member signup forms.
   Existing requests without that field remain valid.
2. Prepare the local worker and public VAPID key early. On a signup gesture, if
   permission is granted and the device is not explicitly off, obtain the
   existing subscription or create one automatically.
3. Pass that subscription with the signup request. Do not send a separate
   registration request after a successful signup as the normal path.
4. Validate the optional subscription before use. Inside the signup transaction,
   bind the device only when the actual outcome is `AddedWaitlist`.
5. Commit the booking and valid device binding together. A cancellation that
   runs next can now find the recipient device.
6. If the outcome is `AddedPlayer`, do not create a new binding just for that
   signup. Existing registrations remain unchanged.

Use HTMX 2.0.8's `htmx:confirm` asynchronous continuation for signup preparation,
then `htmx:configRequest` or form fields for the prepared payload. Check the
current documented event behavior. Do not replace all booking requests with
custom fetch calls.

Integrate preparation into the existing shared pending state:

- Prevent duplicate clicks, other booking writes, refreshes, and local identity
  changes during preparation and the eventual request.
- Preserve roster expansion, focus, error handling, and request-button geometry.
- Keep `X-Hoopline-User` tied to the originally displayed member. A cross-tab
  identity change must still fail before any database mutation.
- Bound non-interactive push preparation, for example to three seconds. On
  failure, submit the ordinary booking once without push data. Never cancel,
  retry, or move the booking to restore a queue position.
- Ignore late preparation results after timeout, navigation, or identity change.
  Do not register the wrong member or issue the signup twice.
- Permission `default`, `denied`, explicit off, disabled server configuration,
  or unsupported APIs must use the ordinary booking path without waiting.
- Do not intercept cancellation/admin confirmations or accidentally bypass them.

Browser setup failures and delivery failures must not fail booking. Invalid
optional push fields must not lose a valid signup; return separate alert setup
feedback. Unexpected SQLite errors still roll back the whole transaction. Do
not swallow database errors or claim that either change was committed.

For the explicit Enable action after permission grant, use a separate
registration route. Reuse device ownership and `BookingIdentity` checks.
After registration, refresh current booking state. A promotion before this
registration is not replayed; the page shows the current confirmed state.

## 7. Server Contract and Security

Suggested routes:

| Route | Purpose |
| --- | --- |
| `GET /push/status` | Public VAPID key, feature availability, and this device's state for the selected member |
| `POST /push/subscribe` | Register after an explicit permission grant or retry |
| `POST /push/disable` | Persist off for this device and cancel its pending jobs |
| `POST /push/test` | Enqueue one test alert for this owned, active registration |

Status must not create a member binding or expose another device. A missing or
mismatched binding is “not registered,” not a member-wide device listing.
Make subscribe/disable idempotent. Disable must work for the owned device even
when its old member binding has been detached.

Use same-origin JSON or form requests. Binding and test-send routes require
`BookingIdentity` and the separate device token. Disable requires device ownership.
Check `Origin` against a configured public app origin on push writes. Reject
cross-origin requests. Do not trust a client-supplied member ID or arbitrary Host
header as the origin policy.

Push endpoints are untrusted network destinations:

- Bound request, endpoint, and key lengths. Decode and validate subscription keys.
- Require HTTPS with no credentials, fragment, nonstandard port, or IP literal.
- Use an explicit supported-provider host allowlist. Verify current Apple,
  Chromium, and Firefox endpoints during implementation. Do not use broad
  substring matches or accept arbitrary custom endpoints.
- Disable redirects. Do not fetch endpoint URLs during signup validation.
- Apply the same checks before send. Keep network timeouts and response limits.
- Prevent access to local/private destinations, including through DNS
  resolution. Do not permit an unsafe test endpoint in production.

Apply registration origin and ownership checks to the optional signup payload
as well as to `/push/subscribe`. Do not create a weaker registration path inside
the booking handler.

Add a separate push rate-limit category so retries cannot exhaust the booking
limit. Suggested limits: setup/disable 10 per minute and 30 per hour per IP;
test sends 1 per minute and 5 per hour per device, plus an IP limit.
Limit repeated registration attempts and total stored devices.

The identity policy remains trust-based. A visitor can still select someone
else's member name and register their own device for it. Document this limit.
Device ownership prevents changing another device, not member impersonation.
Do not add personal names or contact information to lock-screen payloads.

## 8. Promotion Jobs and Delivery

In `promote_first_waiter`, add job creation only in the existing member-action
branch where `actor_user_id` is present. Enqueue after the roster update, before
commit, with the promoted booking ID. Preserve activity logging and queue order.
An outbox insert failure rolls back cancellation, promotion, and history together.

Use the outbox, not polling of `activity_log`. Do not create jobs from
`promote_waitlist_checked`, admin removal, or generic roster update helpers.

Worker requirements:

- Run a small bounded Tokio loop from application startup, alongside renewal.
  Start promptly and inspect pending work about every five seconds.
- Claim due jobs in a short SQLite transaction. Use a lease and claim token so
  overlapping app processes cannot normally send the same job concurrently.
  Recover expired claims after restart. Never hold a DB transaction over HTTP.
- Bound each send to ten seconds. Keep concurrency small, initially one.
- Before each send, verify that the device is enabled, its member and generation
  match, the exact booking still belongs to that member in Players, and the game
  has not started. Use current game time and venue for the message.
- Skip obsolete jobs. Cancelling and rebooking must not revive an old job.
- Set expiry to the earlier of game start or 30 minutes after promotion. Set the
  provider TTL from remaining expiry, not a fixed long retention.
- Retry network failures, timeouts, HTTP 429, and HTTP 5xx with bounded backoff.
  Start with 10 seconds, 30 seconds, two minutes, five minutes, and 15 minutes.
  Respect `Retry-After` and expiry. Stop after six total attempts.
- Treat 404/410 as expired subscriptions; remove their delivery credentials and
  stop their pending jobs. Surface setup recovery on the next visit.
- Do not retry invalid payloads indefinitely. Treat VAPID 401/403 as server
  configuration errors, not proof that every endpoint is expired.
- Record provider acceptance, not “delivered” or “read.” Redact network secrets.
- Guard acknowledgments with the claim token. Keep terminal jobs for seven days,
  then remove them. Bound cleanup batches.
- Close cleanly when its pool closes. Reuse `src/clock.rs` for application time.

Delivery is at-least-once. A crash after provider acceptance can cause a retry.
Use a stable browser notification tag such as `promotion:<booking_id>` to reduce
duplicate visible alerts. Do not promise exactly-once display.
Disable/logout can prevent queued sends, but cannot recall an accepted push.
The notification must tell the player to check the current roster.

## 9. Service Worker Behavior

Example content:

> You're playing! 🏀
>
> You moved off the waitlist for Thursday at 20:00 at Court A.
>
> Open Hoopline to check your booking.

Use Berlin-local dates and times, consistent with the app. Keep payloads below
provider size limits and render plain text, not HTML.

- Show a user-visible notification for each received push. Safari does not
  support invisible push. Do not use push for background roster refresh.
- For malformed payloads, show a safe generic notification instead of crashing
  or treating the event as a silent push.
- Permit only Hoopline roster destinations, such as `/slots#slot-123`.
  Do not open payload-supplied external URLs.
- On click, close the notification. Focus/navigate an existing suitable Hoopline
  window, or open one. Normal page refresh loads current state.
- Do not switch the selected member, create admin sessions, or perform booking
  mutations from the notification. If no member is selected, use normal selection.
- Refresh a focused roster through the existing focus/visibility flow.

## 10. Configuration and Deployment

Use explicit configuration, for example:

- `PUSH_ENABLED`, default `false`.
- `VAPID_PUBLIC_KEY`.
- `VAPID_PRIVATE_KEY` in one documented format supported by the selected library.
- `VAPID_SUBJECT`, a valid operator `mailto:` or HTTPS contact.
- `APP_ORIGIN`, the canonical HTTPS origin used for push route checks.

With push disabled, keep the PWA and all booking functions available. Do not
register devices or send jobs. Expiry must still prevent stale sends after
re-enabling. With push enabled but invalid configuration, fail startup with a
clear redacted error. Do not silently advertise working alerts.

Keep VAPID keys stable across deployments and backed up securely. Never generate
replacement keys on every startup. Document that key rotation can require device
resubscription. Public keys are public; private keys and subscription credentials
are secrets. No Apple developer account or hosted push vendor is required.

Check TLS/native dependencies in `Dockerfile`. Verify HTTPS, secure cookies,
public origin, outbound provider access, persistent SQLite, restart behavior,
and backups on the host. Do not deploy, change Hostim settings, or send real
notifications to group members without separate approval.

## 11. Implementation Units

Implement in this order. Add tests with each unit.

1. **PWA shell:** manifest, icons, worker registration, install guidance,
   online-only behavior, and static routes.
2. **Push foundation:** configuration, additive migration, device ownership,
   endpoint validation, sender selection, status/enable/disable/test routes.
3. **Automatic waitlist enrollment:** signup payload, atomic device binding,
   permission UX, shared request-lock integration, and honest status feedback.
4. **Member promotion delivery:** transactional jobs, worker claims/retries,
   current-booking checks, payload display, and notification navigation.
5. **Verification and handoff:** full checks, documentation, container build,
   and approved real-device checks.

Keep the feature disabled until the complete path works. Do not enable partial
notification support in production.

## 12. Acceptance Tests

### Rust/database/HTTP

- Fresh migration and upgrade from migration 011 preserve bookings and history.
- Valid granted-permission signup binds the device only for an actual waitlist
  result. Player signup, full lists, stale identity, and rejected writes do not.
- Waitlist booking and registration commit atomically. A concurrent cancellation
  immediately after signup finds the device and creates one job.
- Existing clients without push fields still book. Invalid optional push data
  produces a saved booking with separate setup failure feedback.
- Member cancellation creates one job per eligible device for the first waiter.
  All “No” rows in the trigger table create no promotion jobs.
- Duplicate cancellation/retry, rebooking in the same second, and migration of
  existing booking IDs do not duplicate or misassign jobs.
- Cancellation, promotion, activity, and jobs all roll back on an insert failure.
- Ownership mismatch cannot update, disable, test, or steal another device.
  Stale displayed identity and cross-origin writes are rejected.
- Endpoint SSRF cases, invalid keys, input size, redirects, rate limits, and
  device caps are covered without sending to the public internet.
- Logout/name changes detach only this device and invalidate old pending jobs.
  Explicit off survives page reloads and later joins.
- Restart resumes jobs. Two workers cannot hold a valid claim simultaneously.
  Test expiry, backoff, `Retry-After`, claim recovery, and 404/410 cleanup.
- Cancelling/rebooking, deleting records, disabling, rebinding, or game start
  suppress obsolete jobs. A game edit before send uses current details.
- Disabled/misconfigured push and production rejection of test overrides work.

### Browser scenarios

Use the existing isolated real-app fixtures. Add a PWA-specific test group with
`serviceWorkers: 'allow'`; keep other tests blocked as today. Keep all automatic
tests independent of real browser-provider credentials and public network access.
Pass this option through to the fixture's `newActor` contexts too; that helper
currently hard-codes `serviceWorkers: 'block'`.

Test real worker registration, scope, manifest, icons, and local asset serving.
For permission/subscription states, use a narrow browser API stub where necessary.
Use an injected sender or a compile-gated local test sender to inspect server
jobs and simulate provider errors. Do not weaken production endpoint checks or
add public reset/send-anywhere endpoints. Stubbed tests do not prove OS delivery.

Cover these user outcomes:

1. Permission already granted, no browser subscription: join a full game;
   registration happens automatically with no Subscribe click.
2. Granted permission and an existing subscription: reuse it without duplicate
   registration. A stale Sign Up that becomes a waitlist also enrolls.
3. Default permission: waitlist succeeds; one Enable action grants permission and
   registers. Denial and unsupported APIs leave booking usable.
4. Setup timeout/failure: exactly one booking succeeds. Show alerts unavailable,
   allow setup retry, and ignore late callbacks.
5. Explicit off: later waitlist joins do not silently turn alerts back on.
6. Immediate promotion after signup: the registered device has a queued job.
7. Cancellation/promotion/rebooking, name changes during preparation, stale tabs,
   focus refresh, slow requests, and lost responses preserve roster correctness.
8. Worker push display, stable notification tags, malformed payload fallback,
   internal-only click URLs, and opening the current roster.
9. Keyboard use, announced feedback, touch targets, contrast, and narrow layouts.
10. No roster/identity entries in Cache Storage. Offline booking never reports
    success. Worker updates do not reload or discard an open booking form.

### Real hardware / hosted HTTPS

Before claiming notification support, test with consenting testers on:

- An installed iPhone PWA, with Hoopline closed and the phone locked.
- Android Chrome, installed and not installed.
- At least one supported desktop browser.

Verify permission grant/denial, test alert, real member-triggered promotion,
notification tap, correct local game time, logout, name change, restart, and
permission revocation. Check iPhone installation from a browser tab and selection
inside the installed app. Check Focus settings when delivery is not visible.
Record device, OS/browser version, and observed results.

## 13. Checks and Handoff

Check `Justfile` again before running commands. On this baseline:

```sh
npm ci
npx playwright install chromium
just test-all
cargo build --locked
git diff --check
```

Run build/test commands sequentially as required by `docs/testing.md`. Verify the
normal production container build if the chosen sender changes system libraries.
Keep CI offline from real push services. Run normal-build tests proving that
scenario-only sender or clock settings are ignored.

Update `README.md`, `docs/testing.md`, and the current plan status. Replace the
unconditional “no notifications are sent” member waitlist text with an accurate
permission/device explanation. Keep admin “no notification” warnings accurate.
Document automatic enrollment, explicit off, no admin-triggered alerts,
installation limits, configuration, retries, and best-effort delivery.

Handoff must list implemented behavior, automated check results, real-device
results or untested items, deployment requirements, and remaining identity risks.
Do not claim a Chromium permission stub proves iPhone delivery.

## 14. Reference Documents

Checked during planning on 2026-11-13. Check current library documentation again
when implementing exact APIs and dependency features.

- [Apple: Sending Web Push notifications](https://developer.apple.com/documentation/usernotifications/sending-web-push-notifications-in-web-apps-and-browsers)
- [WebKit: Web Push for iOS/iPadOS Home Screen web apps](https://webkit.org/blog/13878/web-push-for-web-apps-on-ios-and-ipados/)
- [MDN: Push API](https://developer.mozilla.org/en-US/docs/Web/API/Push_API)
- [MDN: PushManager.subscribe](https://developer.mozilla.org/en-US/docs/Web/API/PushManager/subscribe)
- [MDN: Web app manifests](https://developer.mozilla.org/en-US/docs/Web/Progressive_web_apps/Manifest)
- [HTMX: htmx:confirm](https://htmx.org/events/#htmx:confirm)
- [Rust Web Push](https://github.com/pimeys/rust-web-push)
