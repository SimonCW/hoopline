# Hoopline Refinements (Post-Milestone 8)

## Current Status and Next Tasks (2026-11-04)

- The `finish-admin-experience` branch implements admin roster controls, single-game editing, recurring defaults, and the Sunday release. Validate and review this branch before deployment; the no-auth and production safety tasks remain open.
- Admin roster actions now run after a game starts; only the game editor keeps the started restriction. Member rules are unchanged. Approved for simplicity at about 20 requests per day.
- [PR #1](https://github.com/SimonCW/hoopline/pull/1) merged signup, cancellation, waitlist routing, and automatic promotion.
- [PR #2](https://github.com/SimonCW/hoopline/pull/2) merged the mobile member flow, rolling schedule, accessibility and browser tests, and stale-identity protection.
- The PR checks are historical results. A merge does not verify production deployment, backup recovery, or real-device usability.

### Next: Admin Experience, Then a Safe User Trial

1. [ ] Build the admin experience described in R1. Keep admin authentication out of scope for this work.
2. [ ] Make SQLite backups safe: remove the live-file `cp` fallback, require a consistent backup method, and test failure and restore paths. Verify persistent storage and restore a backup in the deployment environment.
3. [ ] Confirm the no-auth policy with the group, including the ability to select an admin identity, before inviting users.
4. [ ] Separate sample data from production setup. Verify the deployed app, HTTPS cookie settings, legacy slot times, and persistence across a restart.
5. [ ] Run the flow with members on real iOS and Android devices and with a screen reader. Verify the agreed Sunday release, Berlin times, interface language, and card layout.
6. [ ] Add repeatable CI checks and define a required Rust toolchain. Reassess R3 and R4 after the admin experience is in use.

## Goal

Improve the normal member experience first. Focus on clear slot information, simple booking actions, and mobile use. Delay admin editing work until the member flow is ready.

---

## Repository Review and Member UX Priorities (2026-11-03)

### Status at Review (Before Member UX Changes)
- The core MVP is implemented: name selection, self-registration, signup, cancellation, automatic waitlist promotion, and admin removal/promotion.
- SQLite persistence, Docker deployment files, and a backup script exist. The live deployment and backup restoration were not verified.
- R2 layout tasks are complete. R1, R3, and R4 remain open and are now deferred.
- Rust stable was updated to 1.99.1 on the review machine. Build, formatting, Clippy, and all 22 tests passed.
- The local browser check showed no horizontal overflow at 390px width. This does not replace a full mobile usability test. The favicon is missing.
- Slot generation runs at startup or through an admin action. There is no periodic renewal job. Past slots remain visible and booking endpoints do not reject past slots.
- Schedules and displayed times use UTC. German local time and daylight-saving rules need a clear decision.
- Capacity is enforced from database values, but the UI assumes 15 players and 5 waitlist places.
- Identity selection has no authentication. Anyone can select an admin identity. This matches the original plan but does not prevent impersonation.
- Sample users, slots, and bookings are part of the initial migration. Separate demo data from production setup before a user trial.

### Recommended Work Order

Checked member UI items were merged in PR #2. Admin experience is the next feature. Preference decisions and real member testing remain open.

#### 1. Make Slot Information Reliable
- [x] Show upcoming slots only. Reject new bookings for slots that have started.
- [x] Renew the rolling 14-day booking window automatically, checked every minute.
- [x] Generate and display new slots in `Europe/Berlin`, including daylight-saving changes.
- [x] Use database capacity values for counts, labels, and available places.
- [x] Confirm release rule: Sunday at 19:00 in `Europe/Berlin` opens the week starting eight days later. Example: 8 November 2026 opens 16–22 November.
- [ ] Replace Rolling/14 generation with the agreed Sunday rule in admin increment 3. Preserve already-created games and bookings.

#### 2. Make the Mobile Slot List Easy to Scan
- [x] Group slots by week and keep chronological order. Use clear date, time, and venue labels.
- [x] Show availability at the top of each card, such as `9/15 players` and `2/5 waitlist`.
- [x] Remove empty rows and offer expandable rosters. Preserve expanded rosters after booking updates.
- [ ] Test compact cards against the spreadsheet-style view with real members.
- [x] Show a clear booking status on each card: not booked, confirmed, waitlist position, or full. Do not rely on row color alone.
- [ ] Use one consistent interface language. Confirm whether members prefer German.

#### 3. Make Identity and Booking Actions Clear
- [x] Require name selection before the overview. Show only the selected name and `Log out` at the top right during booking.
- [x] Add a fuzzy-search dropdown with explicit pointer or keyboard selection. Keep new-user creation behind an explicit action to reduce accidental duplicate identities.
- [x] Remember the selected user ID for one year with an `HttpOnly`, `SameSite=Lax` cookie. Logout clears the cookie without cancelling bookings. Support `COOKIE_SECURE=true` for HTTPS deployments.
- [x] Keep the primary action near the slot summary: `Sign up`, `Join waitlist`, or `Cancel booking`.
- [x] Explain waitlist behavior before joining. State that cancellation automatically promotes the first person in line.
- [x] Show local success feedback after each action, including the resulting booking status or waitlist position.
- [x] Prevent repeated submissions while a request is pending. Keep errors near the affected slot and announce status changes to screen readers.
- [x] Protect against accidental cancellation with a short confirmation. Do not promise undo unless queue fairness can be preserved.

#### 4. Test the Complete Member Flow
- [x] Test name selection, signup, waitlisting, cancellation, and promotion in Chromium with phone and desktop layouts.
- [x] Test keyboard navigation, focus restoration, automated contrast/accessibility rules, long names, and touch targets. Check overflow at 320px, 390px, and 430px.
- [x] Refresh booking state and identity when the page regains focus. Add a manual refresh action.
- [x] Test simultaneous signups with a file-backed database. Serialize writes and show retry feedback for database conflicts.
- [x] Test slow networks and failed requests. Confirm that failed actions do not show success or change the displayed booking state.
- [ ] Check the flow on real iOS and Android devices and with a screen reader. Automated checks do not replace this test.

### Implementation Notes
- New schedules interpret `20:00` as Berlin local time. Existing slots keep their stored instant and bookings. Generation avoids creating another slot at the same venue on that local date.
- Existing legacy slots can therefore show a different local time until reviewed. Do not silently move an existing booking.
- Keep English action labels and the existing German short date format for now.
- Remove the visible time-zone label. Remembered users skip name selection. Visitors without a valid identity cookie cannot see the overview. Returning to an open overview after logout in another tab returns to name selection.
- Keep the overview header compact: Hoopline first, Log out at the top right, and `You're up next, Name` below. Highlight the name with a high-contrast badge instead of a separate identity row.
- Use local CSS instead of runtime Tailwind/CDN styling. HTMX remains pinned to its existing CDN version.
- Browser tests use an isolated in-memory server on port 5051. They do not change the local preview or production database.
- Verification passed: build, formatting, Clippy, 42 Rust tests, and 52 phone/desktop browser tests. Automated accessibility scans cover name suggestions, name forms, rosters, and confirmed booking state.
- Admin editing milestones remain deferred. Existing admin endpoints remain available.

### PR Review Fixes
- Keep every booking action, admin booking action, and refresh button disabled while the shared request runs. Keep new controls disabled after HTML replacement until the update settles. Announce request status to screen readers and restore controls after success or failure.
- Submit the selected member ID from the name dropdown. Clear that ID when the search text changes. Reject unselected names and existing names submitted through the new-member form instead of choosing an account by case-insensitive lookup.
- Regression tests reproduced both review findings before the fixes. Tests now cover signup and cancellation during a delayed refresh, controls on other cards, HTML replacement, failure recovery, selection changes, and legacy names such as `Ben` and `ben`.
- Prevent actions under a stale displayed identity after a failed refresh. Each action sends the rendered member ID. The server rejects missing or mismatched IDs before making changes. Refresh and logout remain available.
- Use private-field `BookingIdentity` and `AdminIdentity` extractors in `src/identity.rs`. Runtime validation happens once at the request boundary. Handler signatures then require a validated member identity and, for admin routes, the database admin flag. Do not replace unrelated database IDs with newtypes.
- Browser regression tests reproduce a failed focus refresh after another tab selects a different member. Verify that signup and cancellation change neither account, then verify recovery after a successful refresh. Server tests also cover admin actions, missing/invalid metadata, and logged-out cookies.

### Quiet Updates Without Layout Flicker
- Do not insert visible loading banners or change button labels and sizes during requests. Keep loading announcements available to screen readers.
- Use a clear gray disabled-button style. A permanently reserved refresh icon rotates through CSS only while the overview refreshes or settles. Keep it still for reduced-motion preferences. Do not add loading timers or separate JavaScript state for visual feedback.
- Use the existing booking-status badge to confirm success. Keep detailed success messages for screen readers instead of adding another visible message to the card. Keep errors visible.
- Restore expanded rosters immediately after HTML replacement, before the next frame. Do not wait for the settle delay.
- Geometry regression tests cover header/card/button positions during requests, expanded rosters in the first refreshed frame, and card size after a successful booking.

### Before a Real User Trial
- [ ] Verify persistent storage and restore a backup. Remove the unsafe live-file copy fallback from the backup process.
- [ ] Confirm the no-auth access policy, especially admin identity selection.
- [ ] Separate sample data from production data and verify the deployed app.
- [ ] Add repeatable CI checks and define the required Rust toolchain. The README records the toolchain used for local verification, but does not pin one.

---

## Milestone R1: Admin Mode and Slot Editing

Implementation checklist: [`admin_experience.md`](admin_experience.md). Ship three increments: admin mode/rosters, individual game editing, then recurring defaults/Sunday release. Read remaining product decisions before implementing proposed behavior. Use small browser workflows and detailed Rust tests; do not add custom test-process orchestration or dirty-form/navigation tracking.

**Goal:** Keep the member layout and add frequent admin actions in a clearly distinct admin mode.

### Tasks
- [ ] R1.1: Add an **admin mode toggle** (e.g., segmented control):
  - User View (default): behaves like a normal member view
  - Admin View: enables admin editing controls
- [ ] R1.2: Use distinct colors and a clear `Admin Mode` label without changing the familiar slot layout.
- [ ] R1.3: Move a waitlisted member up or down. Player ordering has no special meaning.
- [ ] R1.4: Remove a member from either list. Preserve automatic promotion when a player is removed.
- [ ] R1.5: Add an existing member to the end of the selected player list or waitlist. Enforce capacity and prevent duplicate bookings.
- [ ] R1.6: Edit a slot date, time, and location. Ask whether to keep or clear bookings only when bookings exist. Reject a stale save if a booking arrived after the form was loaded.
- [ ] R1.7: Preserve slot and waitlist invariants, handle conflicting changes, and test all actions and failure paths.
- [ ] R1.8: Add a separate `Edit recurring schedule` action at the top of admin mode. Keep it separate from `Edit this game` on each slot card.
- [ ] R1.9: Let admins add or pause a recurring game and change its weekday, time, default location, and capacity. Apply changes to newly created games only. State clearly that existing games and bookings do not change.
- [ ] R1.10: Replace Rolling/14 with one shared Sunday release at 19:00 Berlin time. Release the week starting eight days later, as in the 8 November → 16–22 November example. Only release time is editable; do not add policy variants, weekday selection, or week offsets. Show the next release and target week.

### Behavioral Rules
- [ ] Admins in **User View** should not see admin editing controls.
- [ ] Roster actions use server-confirmed card updates. Editor saves return to the full overview or schedule page. Disable submit controls while saving; do not add navigation locks or unsaved-change warnings.
- [ ] Clearing a player row should preserve queue fairness by applying existing promotion/shift logic.
- [ ] Changing a recurring slot must not cause the renewal job to recreate the original slot. Define and test how the edit interacts with the recurring schedule.
- [ ] `Edit this game` changes one occurrence only. Mark changed occurrences clearly and preserve them during automatic renewal.
- [ ] Recurring schedule edits affect newly created games only. Existing games need an explicit occurrence edit, including the choice to keep or clear bookings.
- [ ] Keep the booking opening policy separate from recurring game defaults. Use one shared policy for all recurring games.
- [ ] Admin identity selection remains unauthenticated. Admin mode is not a security boundary.

### Test Criteria
- **Manual:**
  - Admin can switch views and clearly see which mode is active.
  - In Admin View, add and remove members, reorder the waitlist, and edit a slot.
  - The slot editor asks what to do with bookings and applies the chosen action.
  - User View stays clean and behaves like regular member mode.
- **Auto:**
  - Non-admin users cannot call admin endpoints (403).
  - Add and reorder actions enforce capacity, uniqueness, and waitlist ordering.
  - Slot edits preserve or clear bookings as requested and survive schedule renewal.
  - Recurring schedule changes leave existing games and bookings unchanged.
  - All recurring games use the same booking window rule, including release boundaries and daylight-saving changes.
  - Mode toggle state is respected in rendered UI.

---

## Milestone R2: Slot Layout & Action Visibility

**Goal:** Make player/waitlist columns readable on real phones and ensure controls are fully visible.

### Tasks
- [x] R2.1: Widen player/waitlist presentation so names and action controls are not clipped.
- [x] R2.2: Rework slot card layout for mobile first (stacking/scroll behavior with larger tap targets).
- [x] R2.3: Remove the `"You"` badge text and rely on row highlighting only.
- [x] R2.4: Ensure action buttons (cancel/admin actions) remain fully visible at narrow widths.
- [x] R2.5: Add responsive breakpoints for compact, tablet, and desktop slot layouts.
- [x] R2.6: Format slot date/time in a human-friendly **German short style** with weekday, e.g. `Do, 05.05.26`.

### Test Criteria
- **Manual:**
  - On phone width, both columns are readable and controls are fully visible.
  - Selected user is clearly highlighted without extra `"You"` labels.
  - No horizontal clipping of cancel/admin controls.
  - Slot dates show localized German-style labels with day-of-week prefix (e.g. `Do, 05.05.26`).
- **Auto:**
  - Render tests assert expected classes/markup for responsive layout variants.
  - Date formatting tests verify German short output and weekday abbreviations.

---

## Milestone R3: Faster Admin Editing Workflow

**Goal:** Make weekly slot correction/editing efficient with minimal clicks.

### Tasks
- [ ] R3.1: Add per-slot “Save all changes” workflow (batch update) as an alternative to per-row auto-save.
- [ ] R3.2: Add dirty-state indicator per slot card (unsaved edits).
- [ ] R3.3: Add reset/revert action to discard unsaved edits.
- [ ] R3.4: Add optimistic UI feedback + error rollback for failed updates.

### Test Criteria
- **Manual:** Admin can update multiple rows quickly and commit once.
- **Auto:** Batch update applies atomically and preserves slot consistency.

---

## Milestone R4: Operational Safety for Admin Changes

**Goal:** Reduce risk of accidental disruptive edits.

### Tasks
- [ ] R4.1: Add lightweight audit log for admin reassignment actions (who, slot, before/after, timestamp).
- [ ] R4.2: Add “undo last change” per slot (time-limited).
- [ ] R4.3: Surface recent admin actions in a compact history panel.

### Test Criteria
- **Manual:** Admin can review and undo recent changes.
- **Auto:** Undo restores previous consistent state exactly.

---

## Notes

- Keep the user path simple: members should still only select their identity and sign up/cancel their own booking.
- Admin controls should be intentionally gated behind explicit mode switching.
- Build R1 next. Reassess R3 batching and R4 audit features after members use the admin flow.
