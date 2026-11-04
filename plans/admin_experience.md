# Admin Experience: Implementation Plan

Status: simplified after user review, 2026-11-04. This file is the implementation checklist for R1 in `refinements.md`.

## 1. Outcome and Scope

Keep the current slot cards. Add an explicit admin mode with distinct colors and a text label.

Confirmed requirements:
- Move members up or down in the waitlist. Player order on non-waitlist has no special meaning.
- Remove any member, including the selected admin, from either list.
- Add an existing member to the end of the selected list. Do not select a position.
- Edit one game's date, time, and location. Ask whether to keep or clear its bookings.
- Edit recurring game defaults on a separate screen.
- Use one shared booking-opening rule for all recurring games.
- Open a whole game week on Sunday two weeks in advance: Sunday 8 November 2026 releases games for Monday 16 through Sunday 22 November 2026.
- Release time is Sunday at 19:00 in `Europe/Berlin`.
- Keep authentication work out of scope. Keep the existing database admin-role and stale-identity checks.

Do not add drag-and-drop, batch roster edits, undo, audit history, notifications, member authentication, or a new frontend framework.

## 2. Proposed Product Rules

These defaults make the work implementable. They are proposals, not additional confirmed user requirements. Keep them visible in the UI and in the PR description.

### Remaining Decisions

The user approved the simpler three-increment delivery, browser tests, editor behavior, and delayed schema changes. Before roster implementation, confirm direct player additions ahead of waiters, waitlist additions when player space exists, and the proposed started-slot restriction. Confirm field limits and new/pause controls before the schedule editor. The Sunday release rule and initial time (19:00 Berlin) are settled. Do not build a policy selector or configurable weekday/week offset.

### Modes and Screens

- An admin starts in Member view. A two-choice control switches between Member view and Admin mode.
- Use `/slots?mode=admin` for admin overview URLs. A URL parameter is view state, not permission.
- A non-admin never receives admin controls. Existing admin POST routes still require `AdminIdentity`.
- Admin mode uses a contrasting accent, an `Admin mode` label, and the same cards and roster structure.
- In admin mode, replace the admin's personal signup/cancel action area with `Edit this game`. Roster controls can still add/remove the admin as a member.
- Keep regular roster expansion. Do not expand every game on entry.
- Put `Edit recurring schedule` once above the cards. Its page has a `Back to admin games` link.
- Put the shared `Sunday release time` form on the recurring schedule page, separate from each weekly game's form.

### Roster Rules

- Add only existing users. Use a labelled native member select first; do not reuse the identity combobox with its fixed IDs or sign-in behavior.
- Exclude already-booked users from the select. Validate again on the server.
- Choose `Players` or `Waitlist` explicitly. Each list has its own capacity. Adding to a full list fails; never silently change lists.
- Adding to Players while others wait is an explicit admin override. Show this warning when a waitlist exists.
- Allow adding to Waitlist even if Players has space. Explain that automatic promotion happens when a player is removed, not merely because space exists.
- Remove uses a confirmation. Removing a player retains the existing first-waiter promotion behavior. Removing a waiter closes the queue gap.
- Keep the existing Promote action when player space exists. It appends to Players and closes the queue gap.
- Up/Down swaps with the adjacent waiter only. Disable Up at the top and Down at the bottom. Server rejects an invalid edge move without changing data.
- All roster mutations reject started slots. Keep legacy member cancellation behavior unchanged unless a separate change is approved.

### Edit This Game

- Use a separate editor page, not a nested form inside a card: `/admin/slots/{id}/edit`.
- Show current values, booking counts, and three fields: local date, local time, and location.
- Use `Europe/Berlin` on the form, including a visible explanation. Reject missing or ambiguous DST times. Do not guess an offset.
- Ask `Keep all bookings` or `Clear all bookings`, with no default, only when the game has bookings. Recheck inside the save transaction. If a booking arrived after the empty form was loaded, reject the stale save and ask the admin to reload. An empty game needs no disposition choice.
- Clear explains that both player and waitlist bookings are removed and no notifications are sent. Require confirmation before the destructive submit.
- Only a real date/time/location change applies the keep/clear choice. A no-op save must never clear bookings.
- Reject a new time in the past. Trim location; require 1–120 characters.
- Mark an edited occurrence `Changed game`. Preserve the original occurrence identity so renewal cannot recreate it.
- Save returns to the full admin overview, which restores correct week grouping and ordering. Do not return only a card after a date change.

### Recurring Defaults

- Show one form per weekly game: weekday, Berlin local time, default location, player capacity, waitlist capacity, active/paused.
- Allow a new weekly game. Pause rather than delete existing schedules.
- Proposed limits: players 1–100; waitlist 0–100. Use identical server and form limits.
- Validate recurring local times as `HH:MM`. A recurring time can be valid on ordinary dates but missing or ambiguous on a Berlin DST date. Skip that occurrence, report its schedule/date, and continue generating other games. Never guess an offset or roll back all schedules for this expected case.
- Save copy: `Changes apply only to games created after this save. Existing games and bookings stay unchanged.`
- Pausing stops creation only. Existing games remain visible and bookable.
- A changed weekday can create another game in the current window. Existing games are not moved or removed. State this in the confirmation summary.
- Do not add an "apply to all existing games" option in this feature.

### Shared Sunday Release

There is one rule, not a choice of policies:

- Every Sunday at 19:00 Berlin time, release the Monday–Sunday week that starts eight days later. Example: 8 November 2026 at 19:00 releases 16–22 November, not 9–15 November.
- For a target week's Monday M, release date is M minus eight calendar days. Use Berlin dates, not UTC-duration subtraction. All recurring schedules use the same Sunday release time.
- The release time is the only editable opening setting. Show the next release date/time and target week beside it. Do not add rolling days, weekday selection, week offsets, or per-schedule rules.
- Initial release time is 19:00. Validate any edited release time against Berlin DST ambiguity before saving; do not add a general time-zone policy system. Game-time editing retains its missing/ambiguous-time validation.
- Generate only open games. Find the newest target week whose release has occurred. Generate future games through its Sunday, including games from earlier open weeks. At the release instant the new target week becomes eligible.
- Keep already-created games and bookings when switching from Rolling/14 or changing the release time. Do not silently withdraw a published game.
- Read schedules and the shared release time each renewal run. Use one pure function with an explicit `now` argument for the release calculation and display example.
- Keep the one-minute renewal loop and startup catch-up. New games appear on the next run, up to one minute after release. The explicit Generate action respects the same cutoff.
- Increments 1 and 2 keep the current Rolling/14 implementation. Increment 3 replaces it with this Sunday rule; do not maintain both modes afterward.

## 3. Current Code Map

- `src/lib.rs`: all routes and handlers; Askama view structs; one-minute renewal setup.
- `src/identity.rs`: `BookingIdentity` and `AdminIdentity`; compares `X-Hoopline-User` with the cookie and database.
- `src/db.rs`: `BEGIN IMMEDIATE` write transactions, signup/cancel/promotion, schedule generation.
- `src/models.rs`: slot data, Berlin display times, weekly grouping.
- `templates/slot_card.html`: current Remove/Promote controls appear whenever the identity is admin. Change this to explicit admin mode.
- `templates/user_selector.html`: current admin-tools section and manual Generate button.
- `assets/app.js`: shared request lock, errors, focus refresh, preserved expanded rosters. Extend rather than replace it.
- `migrations/002_slot_schedules.sql`: recurring configuration and `(datetime, venue)` unique index.
- `migrations/004_local_schedule_time.sql`: `time_utc` renamed to `time_local`.
- `tests/integration.rs`: database/router tests and fixed future fixtures.
- `tests/browser/member.spec.js`: Chromium mobile/desktop member regressions. Tests share one server, sequentially.

## 4. Data and Concurrency Design

Do not modify an applied migration. Add the next available migration number; `005_admin_schedule.sql` is the current candidate.

### Occurrence Identity

Add to `slots`:
- `schedule_id INTEGER NULL REFERENCES slot_schedules(id)`.
- `occurrence_date TEXT NULL`: original Berlin game date, `YYYY-MM-DD`.
- `is_override INTEGER NOT NULL DEFAULT 0 CHECK (is_override IN (0,1))`.
- `revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0)`.
- A unique index on `(schedule_id, occurrence_date)`. SQLite permits multiple NULL pairs for legacy/manual rows.

Each schedule already has a stable primary key in `slot_schedules.id`. Generation must write that ID and the original Berlin occurrence date into each new game in the same insert. The pair identifies the original recurring occurrence. An individual edit changes neither field. A schedule default change does not change existing rows. Pause schedules rather than deleting their provenance. Keep `(datetime, venue)` uniqueness too.

Legacy games stay unlinked:
- Do not infer a creating schedule from date, time, or venue. Do not add a matching backfill or completion-marker table. The creating schedule is unknown for existing games.
- Migration preserves all existing games and bookings with NULL provenance fields. Do not move or relabel them as known recurring occurrences.
- Preserve the current same-venue/local-date duplicate guard for unlinked rows. This conservative compatibility rule lasts only while those dates remain in the generation window; it does not establish schedule provenance.
- An unlinked game's edit must reserve its old venue/date so renewal cannot recreate it. Add `slot_generation_exclusions(venue TEXT, local_date TEXT, PRIMARY KEY(venue, local_date))`; insert the old pair in the same edit transaction. Do not delete these records in this feature.
- Generation first checks the occurrence pair, then the legacy/exclusion guard, then actual datetime/venue collision. The same-venue/local-date guard applies only to unlinked rows and exclusions. Linked rows must allow two different schedules at different times at the same venue on the same date. Never overwrite an existing game to resolve a collision.
- A collision must not crash the periodic loop or overwrite another game. Skip it and report it in the explicit Generate response; log it during automatic renewal.

### Configuration Data: Add It When Needed

Do not create unused policy tables in the first increment. In increment 3, add a small singleton settings row with `sunday_release_time` and `revision`, seeded with `19:00`. There is no policy kind or variant fields.

Add `revision INTEGER NOT NULL DEFAULT 0` to `slot_schedules` with the schedule editor, not earlier. Schedule and release-time forms submit their respective revisions.

Use enums for list selection, move direction, and booking disposition. Validate form fields before writing. Do not build a generic settings framework or pass arbitrary column names or SQL fragments from form values.

### Transaction Rules

- Every admin write uses `AdminIdentity`, then `BEGIN IMMEDIATE`.
- Do not call `cancel_booking_for_slot` or `promote_waitlist_user` from a new outer admin transaction: these functions start their own transactions. Extract transaction-local helpers for shared mutation steps. Admin wrappers check revision inside their transaction, then call these helpers and increment revision once. Add started-state checks only in Unit C after its approval gate. Member cancellation keeps its current behavior through its own wrapper.
- Every roster or game-edit form includes the rendered slot revision. Compare it inside the write transaction before changing anything.
- Increment slot revision once for every successful booking mutation, including existing member signup/cancel and admin Remove/Promote. Update the old functions in the same change so a member booking invalidates an open destructive editor.
- New admin actions and existing admin Remove/Promote require the revision. Member endpoints can keep their existing request contract; they must still increment revision.
- A stale revision returns HTTP 409 and a clear reload instruction. It changes nothing. Add a distinct conflict error, not the identity-changed message.
- Use the same compare-and-increment rule for recurring defaults and shared release-time saves in increment 3. New schedule inserts do not have a prior revision.
- Slot edits update fields, booking disposition, override flag, exclusions, and revision in one transaction. Check no-op before clearing bookings.
- Schedule/settings writes and renewal use write transactions. Read schedules/settings inside the generation transaction to avoid generation from stale configuration after a save.
- Do not add a process-local mutex. SQLite serializes writes across connections and app instances.
- Duplicate user, missing member, missing game, full selected list, past game, invalid time, and edge movement all return specific errors without partial writes.

## 5. HTTP and Rendering Contract

Use these routes unless an existing repository convention requires a smaller equivalent:

| Route | Action | Success |
| --- | --- | --- |
| GET `/slots?mode=admin` | Admin overview | Full page |
| GET `/slots/fragment?mode=admin` | Admin refresh | Full overview fragment |
| POST `/admin/slots/{id}/add` | `{user_id, list, revision}` | Updated admin card |
| POST `/admin/slots/{id}/move/{user_id}` | `{direction, revision}` | Updated admin card |
| POST existing Remove/Promote | Add `{revision}` | Updated admin card |
| GET `/admin/slots/{id}/edit` | Single-game editor | Full page |
| POST `/admin/slots/{id}/edit` | `{date, time, venue, bookings, revision}` | Redirect to admin overview |
| GET `/admin/schedule` | Recurring defaults and Sunday release time | Full page |
| POST `/admin/schedule` | New schedule | Redirect to schedule page |
| POST `/admin/schedule/{id}` | Defaults/paused flag and revision | Redirect to schedule page |
| POST `/admin/release-time` | `{sunday_release_time, revision}` | Redirect to schedule page |

- Admin GET pages check the database role using the current cookie. Do not require `X-Hoopline-User` on navigation. Admin POSTs always use `AdminIdentity`.
- All editor POST forms use HTMX and an explicit rendered `X-Hoopline-User` header. Keep `AdminIdentity` unchanged. Do not add hidden identity fields or another extractor.
- Editor forms target their own editor panel with `hx-swap="none"` and `hx-sync="body:drop"`. Success returns the existing `identity_redirect` response with `HX-Redirect`. For field errors, return HTTP 400 through `AppError`; the JS error handler extracts the existing error message and displays it inside the editor. No error response replaces the fields. HTTP 409 also preserves fields but requires a visible `Reload latest values` link. Never auto-update a stale revision or retry a destructive save.
- Extend a small render context with `admin_mode`; do not confuse it with `current_user_is_admin`.
- Admin card render context also needs the available member list and revision. Do not fetch users separately for every card; load once per overview or once for a card mutation.
- Preserve `mode=admin` in fragment URLs and focus refreshes. OOB identity-header updates must not reset the selected mode. If the selected identity loses admin status, refresh must remove admin controls or navigate to Member view.
- Keep the current shared lock for overview booking actions. Disable editor submit buttons while saving and restore them on failure. Extend local error handling to `.admin-editor`. Do not add global navigation/logout interception or dirty-form tracking.
- Do not refresh editor pages on focus. Preserve entered fields after errors. A successful save navigates through `HX-Redirect`; it does not need an unlock-before-navigation or dirty-state handler. Failed, aborted, or timed-out requests must restore submit controls. After an uncertain timeout, ask the user to reload and check the saved state before retrying.
- Navigation away does not cancel a server transaction. Never promise that leaving the page cancels a submitted change.
- New controls need labels, visible keyboard focus, adequate tap targets, and local announced errors. Use text as well as color. Waitlist buttons need member-specific accessible names.

## 6. Implementation Units

Complete one unit at a time. Start each unit by reading this plan and the touched code. Add failing tests before changes. Do not implement later units to fix a failing earlier unit.

Ship three independent increments:
1. **Admin mode and roster controls:** B, then roster-only A4 and C. Add the slot revision column and model field here. This does not depend on schedule changes.
2. **Individual game editing:** A1–A3 (provenance/exclusions), then D. Keep Rolling/14.
3. **Recurring defaults and Sunday release:** E and F, with their own migrations. Remove Rolling/14 only in this increment.

Apply G's relevant regression and documentation work to each increment. The unit letters describe work areas, not a requirement to complete all of A before B.

### A. Data Foundation and Generation Identity

These subunits belong to different increments. Add fields only when their consumer is implemented:

- [ ] A1 (increment 2): Add schedule provenance, override fields, and the exclusion table. Do not add settings or schedule revisions yet. Test a fresh database and an actual older database upgraded with its existing users, slots, and bookings. Build the old fixture from earlier migrations rather than calling current `init_pool` first.
- [ ] A2: Write schedule ID and original occurrence date on every generated game. Test that legacy games remain unlinked and unchanged after migration, renewal, restart, and schedule changes. Test that new games retain their creating schedule ID after default edits.
- [ ] A3: Implement stable occurrence-pair generation. Keep Rolling/14 unchanged. Test repeat renewal, same-day/same-venue distinct schedules, collision preservation, linked moved games, and excluded unlinked games. Use SQL fixtures for moves/exclusions here; do not implement Unit D early to run these tests.
- [ ] A4 (increment 1): Add the slot revision column/model field and transaction-local booking helpers. Increment revision for all existing writes; do not require new member request fields. Test successful writes increment exactly once and rejected writes do not increment. Add the Berlin editor formatter later, in D.

Files: new migration; `src/db.rs`; `src/models.rs`; `tests/integration.rs`.

### B. Explicit Admin Mode
- [ ] Parse mode in overview/fragment handlers. Render only for a database admin.
- [ ] Add mode controls and accessible color/label styling. Hide admin row controls in Member view.
- [ ] Preserve mode through card updates, fragment updates, identity changes, and reloads.
- [ ] Keep current Remove/Promote behavior while adding mode. Add revision checks with A4/C before shipping increment 1.
- [ ] Keep the existing Remove/Promote started-slot behavior in this unit. The proposed restriction belongs to Unit C and requires approval.
- [ ] Reuse existing browser tooling. Set up only the separate disposable admin server/config and Justfile wiring described in G before the first admin workflow test. Do not build a custom scenario runner.
- [ ] Tests: default Member view for admins; non-admin forged mode; direct admin route role enforcement; mode persistence; stale identity. Test stale revisions with A4/C.

Files: `src/lib.rs`; `src/identity.rs` if needed; current overview/header/card templates; `assets/app.css`; `assets/app.js`; Rust and browser tests.

### C. Roster Operations
- [ ] Add typed list/direction inputs and transactional add/reorder functions.
- [ ] Add routes and form controls for existing members and adjacent waitlist movement.
- [ ] Include self-removal and self-addition; remove current template exclusions for the admin's own rows in Admin mode only.
- [ ] Apply started-slot rule to all admin roster operations.
- [ ] Tests: append to either list; capacity; duplicate; missing user; queue swap in both directions; edge rejection; removal promotion; admin self-management; stale revision; no partial writes.
- [ ] File-backed concurrency tests: simultaneous admin add/member signup cannot overfill; stale reorder after promotion fails; revision increments remain correct.
- [ ] Coordinate these tests with explicit barriers/channels or a held SQLite write transaction. Do not use sleeps to assume request ordering.

Files: `src/db.rs`; `src/lib.rs`; `src/error.rs`; card template; Rust tests; `tests/browser/admin.spec.js`.

### D. Single-Game Editor
- [ ] Add editor page and HTMX save form with identity header and revision.
- [ ] Ask keep/clear only for booked games; validate Berlin date/time, location, future time, and collisions. Add the Berlin editor formatter.
- [ ] Preserve original occurrence identity/exclusion and mark overrides.
- [ ] Redirect to the full admin overview after success. Preserve fields and show local errors after failure.
- [ ] Tests: keep; clear both lists; no-op never clears; missing choice for a booked game; empty game needs no choice; booking arrives after an empty form was loaded; DST gap/overlap; past date; duplicate datetime/venue; stale editor; moving between weeks; renewal after moving/changing location; form cancel does not write.

Files: new `templates/admin_slot_edit.html`; `src/lib.rs`; `src/db.rs`; shared CSS/JS; Rust/browser tests.

### E. Recurring Defaults Editor
- [ ] Add schedule revisions in a new migration. Do not create policy-variant storage.
- [ ] Add a separate schedule page and forms for new/edit/pause weekly games.
- [ ] Validate all fields and schedule revision; retain existing schedule uniqueness rule.
- [ ] State the future-created-only behavior and the effect of weekday changes and pausing.
- [ ] Test existing games/bookings unchanged; new games use updated defaults; pause/resume; duplicate schedule; invalid capacity; stale save; no user/identity change during editing.

Files: new `templates/admin_schedule.html`; typed schedule DTOs/domain data; `src/db.rs`; handlers; Rust/browser tests.

### F. Shared Sunday Release
- [ ] Add a small pure Sunday-release calculation in `src/scheduling.rs`, with an explicit `now` argument. Keep database access in `db.rs`. Do not introduce a policy abstraction.
- [ ] Add only the singleton release-time/revision settings in a new migration, seeded with `19:00`. No policy kind, rolling-day setting, weekday selector, or week-offset setting.
- [ ] Implement the fixed Sunday rule in section 2 and remove Rolling/14 generation. In particular, 8 November 2026 releases 16–22 November.
- [ ] Use the same functions for preview examples and generation.
- [ ] Add the shared release-time editor and revision handling.
- [ ] Remove hardcoded `14` from generation and update relevant empty-state/help text and Generate labels. Do not change unrelated identity-cookie durations.
- [ ] Rust tests with fixed clocks: the user's exact November example; before/at/after Sunday release; year rollover; Berlin DST changes; invalid release time; shared cutoff across schedules; already-created games stay published; startup catches missed releases; repeat renewal is idempotent; config/renewal write ordering.

Files: `src/scheduling.rs`; `src/lib.rs` module declaration and handlers; `src/db.rs`; schedule template; current help text; tests.

### G. Regression and Documentation
- [ ] Add one complete admin workflow per browser project. Include keyboard use, a failed save, and submit locking. Cover detailed validation combinations, race conditions, and schedule boundaries in Rust tests, not duplicated browser scenarios.
- [ ] Use `tests/browser/admin.spec.js` with a separate Playwright config, disposable server, and port (5052). Exclude admin tests from the member config and add a separate Justfile recipe. `just test-ui` must run both suites by the final handoff.
- [ ] Use the normal test runner and one disposable admin server for the suite. Keep workers=1. Give each workflow/project its own new schedule and uniquely named members. Do not edit shared default schedules. For the shared release-time setting, establish the expected value at workflow start and restore it in `finally`. The database is discarded after the suite. Do not add a custom per-scenario process runner or production reset endpoints.
- [ ] Before schedule creation exists, roster workflows use unique members and remove their own bookings after the test. For game-editing workflows, restore the game's original values. These suites must not depend on the later schedule editor.
- [ ] Check 320px, 390px, and 430px widths and both existing browser projects. Run accessibility checks on admin overview and both editors.
- [ ] Verify a non-admin or stale admin tab cannot mutate data, including direct HTTP calls.
- [ ] Update README, `refinements.md`, and original plan status. Explain the fixed Sunday rule, future-created-only defaults, changed occurrences, and the remaining no-auth risk.
- [ ] Record exact check results in the PR. Do not claim real-device or production checks from Chromium results.

## 7. Checks and Handoff

After each unit: run its focused Rust tests, then `just verify`. Note that `just verify` runs `cargo fmt` and can format files.

For UI units: run `just test-ui` too. Before first browser run, follow README setup. Before final handoff:

```sh
just verify
cargo fmt --check
cargo build --locked
just test-ui
git diff --check
```

Do not push, deploy, rewrite existing migrations, change production data, or change authentication as part of this plan without a separate request.

If a test or product rule conflicts with the code, stop and state the conflict. Do not silently drop a safety check, change the fixed Sunday release rule, or make existing bookings disappear to simplify implementation.

## 8. Review Record

A review subagent challenged the first draft. This revision addresses the main findings:
- After review, the user proposed explicit creating-schedule metadata. The final plan writes provenance at generation and leaves legacy games unlinked. This replaces the proposed matching backfill and completion marker.
- Invalid recurring DST occurrences are reported without blocking other games.
- Editors use one HTMX/identity/error contract, not two competing form designs.
- A later maintainability review removed dirty-form tracking, global navigation/logout locking, and special redirect cleanup. Editors only lock submits, preserve errors, and do not auto-refresh.
- Admin Remove/Promote checks and mutations share one transaction.
- The duplicate guard no longer blocks distinct linked games on one venue/date.
- The user settled the release rule: Sunday at 19:00 Berlin time opens the week starting eight days later. General policy variants were removed.
- Work now ships in three increments. Schema changes follow their feature. Browser tests use a small isolated suite without custom process orchestration; Rust tests cover detailed cases.

This review checked the plan against the current code. It did not run application tests or approve the proposed product choices.
