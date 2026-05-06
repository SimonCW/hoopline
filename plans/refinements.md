# Hoopline Refinements (Post-Milestone 8)

## Goal

Capture follow-up improvements after the current MVP to make admin operations faster, safer, and more practical during real weekly usage.

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
- [ ] R2.1: Widen player/waitlist presentation so names and action controls are not clipped.
- [ ] R2.2: Rework slot card layout for mobile first (stacking/scroll behavior with larger tap targets).
- [ ] R2.3: Remove the `"You"` badge text and rely on row highlighting only.
- [ ] R2.4: Ensure action buttons (cancel/admin actions) remain fully visible at narrow widths.
- [ ] R2.5: Add responsive breakpoints for compact, tablet, and desktop slot layouts.
- [ ] R2.6: Format slot date/time in a human-friendly **German short style** with weekday, e.g. `Do, 05.05.26`.

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
- Prefer incremental rollout: ship R1 first before adding batching/audit complexity.
