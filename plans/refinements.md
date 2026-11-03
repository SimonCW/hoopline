# Hoopline Refinements (Post-Milestone 8)

## Goal

Improve the normal member experience first. Focus on clear slot information, simple booking actions, and mobile use. Delay admin editing work until the member flow is ready.

---

## Repository Review and Member UX Priorities (2026-11-03)

### Current Status
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

These are recommendations, not completed features. Member UI and UX take priority over the admin milestones below.

#### 1. Make Slot Information Reliable
- [ ] Show upcoming slots only. Reject new bookings for slots that have started.
- [ ] Renew the booking window automatically without requiring a restart or admin action. Confirm whether slots open on a rolling basis or on Sunday evenings.
- [ ] Confirm the intended local time zone. Generate and display local times correctly across daylight-saving changes.
- [ ] Use database capacity values for counts, labels, and available places.

#### 2. Make the Mobile Slot List Easy to Scan
- [ ] Group slots by week and keep chronological order. Use clear date, time, and venue labels.
- [ ] Show availability at the top of each card, such as `9/15 players` and `2/5 waitlist`.
- [ ] Reduce empty-row space. Keep booked names visible and offer a way to expand full lists if needed. Test this against the group's spreadsheet-style view before replacing it.
- [ ] Show a clear booking status on each card: not booked, confirmed, waitlist position, or full. Do not rely on row color alone.
- [ ] Use one consistent interface language. Confirm whether members prefer German.

#### 3. Make Identity and Booking Actions Clear
- [ ] Use a compact identity header after name selection. Keep a clear `Change name` action.
- [ ] Make existing names searchable. Keep new-user creation behind an explicit action to reduce accidental duplicate identities.
- [ ] Keep the primary action near the slot summary: `Sign up`, `Join waitlist`, or `Cancel booking`.
- [ ] Explain waitlist behavior before joining. State that cancellation automatically promotes the first person in line.
- [ ] Show local success feedback after each action, including the resulting booking status or waitlist position.
- [ ] Prevent repeated submissions while a request is pending. Keep errors near the affected slot and announce status changes to screen readers.
- [ ] Protect against accidental cancellation with a short confirmation. Do not promise undo unless queue fairness can be preserved.

#### 4. Test the Complete Member Flow
- [ ] Test name selection, signup, waitlisting, cancellation, and promotion on phone and desktop browsers.
- [ ] Test keyboard navigation, visible focus, contrast, readable names, and touch targets.
- [ ] Refresh booking state when the page regains focus so returning members do not act on stale availability.
- [ ] Test simultaneous signups with a file-backed database. Handle conflicts with clear retry feedback.
- [ ] Test slow networks and failed requests. Confirm that failed actions do not show success or change the displayed booking state.

### Before a Real User Trial
- [ ] Verify persistent storage and restore a backup. Remove the unsafe live-file copy fallback from the backup process.
- [ ] Confirm the no-auth access policy, especially admin identity selection.
- [ ] Separate sample data from production data and verify the deployed app.
- [ ] Add repeatable CI checks and document the required Rust toolchain.

---

## Milestone R1: Admin Panel Modes + Position Reassignment

**Goal:** Admins can switch between two views: normal user behavior and explicit admin editing mode.

### Tasks
- [ ] R1.1: Add an **admin mode toggle** (e.g., segmented control):
  - User View (default): behaves like a normal member view
  - Admin View: enables admin editing controls
- [ ] R1.2: In **Admin View**, replace per-user cancel/remove actions with **position editors** in each slot.
- [ ] R1.3: Each player/waitlist row gets a dropdown (similar to current user selector) with:
  - Current assigned user preselected
  - All users as options
  - Optional empty value for clearing a row
- [ ] R1.4: Add endpoint(s) to persist reassignment by slot/list/position.
- [ ] R1.5: Keep slot invariants intact (no duplicate user in same slot, contiguous positions, correct waitlist ordering).
- [ ] R1.6: In Admin View, show a clear visual indicator (`Admin Edit Mode`) to reduce accidental edits.

### Behavioral Rules
- [ ] Admins in **User View** should not see admin editing controls.
- [ ] Reassignment should be immediate (HTMX fragment update), without full page reload.
- [ ] Clearing a player row should preserve queue fairness by applying existing promotion/shift logic.

### Test Criteria
- **Manual:**
  - Admin can switch views and clearly see which mode is active.
  - In Admin View, dropdown reassignment updates the slot correctly.
  - User View stays clean and behaves like regular member mode.
- **Auto:**
  - Non-admin users cannot call reassignment endpoints (403).
  - Reassignment enforces uniqueness and stable position ordering.
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
- Ship member UX improvements first. Then resume R1 before adding R3 batching or R4 audit features.
