# Implementation Instructions - Basketball Slot Booking

## Philosophy

Build the **smallest deployable thing first**, then iterate. Each milestone should be:
1. **Deployable** - Can run it and see something
2. **Testable** - Clear pass/fail criteria
3. **Incremental** - Builds on previous milestone

---

## Milestone -1: Deploy Current Minimal Page to Hostim First

**Goal:** Ship the current minimal UI immediately to `hostim.dev` before any further feature work.

### Tasks
- [x] -1.1: Add root `Dockerfile` (default Hostim Dockerfile path) using `cargo-chef` for Rust dependency caching
- [x] -1.2: Use Hostim dashboard -> **Create Service** -> **New App** -> **Deployment Type: Git**
- [x] -1.3: Select repository + branch (or provide Git URL manually)
- [x] -1.4: For private repo, provide Personal Access Token if needed
- [x] -1.5: Keep Dockerfile path as `Dockerfile` unless changed
- [x] -1.6: Set replicas/plan and create app

### Docker Security Checklist (required)
- [x] Multi-stage build (no Rust toolchain in final image)
- [x] Use `cargo-chef` planner/cook stages to speed rebuilds without changing runtime footprint
- [x] Minimal runtime image
- [x] Run app as non-root user
- [x] Expose only required app port (`5050`)
- [x] Do not bake secrets into image; use Hostim environment variables

### Test Criteria
- **Manual:**
  - Hostim app build succeeds from Git
  - App starts and serves `/` successfully
  - Homepage shows the current minimal card/box UI
- **Auto:** Local `cargo test` passes before deployment
- **Auto:** Local `docker build` succeeds with `cargo-chef`-based Dockerfile

---

## Milestone 0: Hello World (Foundation)

**Goal:** Rust/Axum server that serves HTML via Askama templates.

### Tasks
- [x] 0.1: Create basic Axum server that responds to `GET /` with "Hello World"
- [x] 0.2: Add Askama template rendering for the index page
- [x] 0.3: Add HTMX and basic CSS (minimal styling, mobile-friendly viewport)
- [x] 0.4: Add `robots.txt` with `Disallow: /`

### Test Criteria
- **Manual:** `cargo run`, visit `http://localhost:5050`, see styled page
- **Auto:** Integration test that starts server, makes GET request, asserts 200 + contains expected text

### Files to Create
```
src/main.rs          - Axum server setup, routes
templates/base.html  - Base layout with HTMX script tag
templates/index.html - Extends base, shows "Hoopline"
```

---

## Milestone 1: Static Slot Display

**Goal:** Display hardcoded slot data (no database yet).

### Tasks
- [x] 1.1: Create `Slot` struct with fields: datetime, venue, players (Vec<String>), waitlist (Vec<String>)
- [x] 1.2: Create template that renders a list of slots
- [x] 1.3: Each slot shows: date/time, venue, player list (numbered 1-15), waitlist (numbered 1-5)
- [x] 1.4: Use hardcoded test data (2-3 slots with some players)

### Test Criteria
- **Manual:** Page shows slots with players listed, looks reasonable on mobile
- **Auto:** Test that slot template renders correct number of player rows

### Files to Create/Modify
```
src/models.rs           - Slot struct definition
templates/slots.html    - Slot list template
templates/slot_card.html - Individual slot display
```

---

## Milestone 1.5: Styling (Tailwind + DaisyUI)

**Goal:** Add visual styling using Tailwind CSS and DaisyUI component library.

### Tasks
- [x] 1.5.1: Add Tailwind CSS via CDN to base template
- [x] 1.5.2: Add DaisyUI via CDN
- [x] 1.5.3: Style the slot cards (use DaisyUI `card` component)
- [x] 1.5.4: Style player/waitlist as `table` or `list`
- [x] 1.5.5: Add responsive layout (mobile-first, single column on phone)
- [x] 1.5.6: Choose a DaisyUI theme (e.g., `light`, `dark`, `corporate`)

### CDN Setup (in base.html)
```html
<head>
  <!-- Tailwind + DaisyUI via CDN (no build step) -->
  <link href="https://cdn.jsdelivr.net/npm/daisyui@4.x/dist/full.min.css" rel="stylesheet">
  <script src="https://cdn.tailwindcss.com"></script>
</head>
<body data-theme="light">
```

### Test Criteria
- **Manual:** 
  - Page looks styled (not default browser styles)
  - Cards have borders/shadows
  - Buttons look like buttons
  - Readable on mobile (test with browser dev tools)
- **Auto:** None needed (visual only)

### Notes
- CDN approach is fine for 61 users; optimize later if needed
- Can switch to standalone Tailwind CLI for production (smaller CSS)

---

## Milestone 2: SQLite Database

**Goal:** Store and retrieve slots/users/bookings from SQLite.

### Tasks
- [x] 2.1: Add SQLx with SQLite, create migration for schema
- [x] 2.2: Create tables: `users`, `slots`, `bookings`
- [x] 2.3: Seed database with test data (3 slots, 10 users, some bookings)
- [x] 2.4: Replace hardcoded data with database query in slot list endpoint

### Local SQLite File Policy
- Keep local SQLite files out of version control.
- Use a non-committed path such as `tmp/hoopline.db` for local development.
- Ensure `.gitignore` contains `*.db` (or at minimum `hoopline.db`) so local data is never pushed.

### Schema
```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    is_admin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE slots (
    id INTEGER PRIMARY KEY,
    datetime TEXT NOT NULL,  -- ISO 8601 format
    venue TEXT NOT NULL,
    max_players INTEGER NOT NULL DEFAULT 15,
    max_waitlist INTEGER NOT NULL DEFAULT 5
);

CREATE TABLE bookings (
    id INTEGER PRIMARY KEY,
    slot_id INTEGER NOT NULL REFERENCES slots(id),
    user_id INTEGER NOT NULL REFERENCES users(id),
    position INTEGER NOT NULL,
    is_waitlist INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(slot_id, user_id)
);
```

### Test Criteria
- **Manual:** Page still shows slots, but data comes from DB
- **Auto:** 
  - Unit test: insert/query users, slots, bookings
  - Integration test: GET /slots returns seeded data

---

## Milestone 3: User Selection (Cookie-based Identity)

**Goal:** User can select their name from a dropdown, stored in cookie.

### Tasks
- [ ] 3.1: Add endpoint `GET /users` returning all user names (JSON or HTML fragment)
- [ ] 3.2: Add user selector dropdown in header (HTMX to load users)
- [ ] 3.3: When user selects name, store `user_id` in cookie
- [ ] 3.4: Display selected user name in header (or "Select your name" if none)
- [ ] 3.5: Add "new user" flow: if name not in list, can type and create

### Test Criteria
- **Manual:** 
  - Select name → cookie is set → name shows in header
  - Refresh page → still shows selected name
  - Can create new user if not in list
- **Auto:**
  - Test cookie setting/reading middleware
  - Test user creation endpoint

---

## Milestone 4: Sign Up for Slot

**Goal:** User can sign up for a slot (or join waitlist if full).

### Tasks
- [ ] 4.1: Add "Sign Up" button on each slot (visible only if not already signed up)
- [ ] 4.2: `POST /slots/{id}/signup` - adds booking for current user
- [ ] 4.3: Logic: if < 15 players, add to player list; else if < 5 waitlist, add to waitlist; else reject
- [ ] 4.4: Return updated slot HTML fragment (HTMX replaces slot card)
- [ ] 4.5: Show user's position in list (highlight their name)

### Test Criteria
- **Manual:**
  - Sign up for empty slot → appears as player #1
  - Sign up for full slot → appears on waitlist
  - Try to sign up when slot + waitlist full → shows error
- **Auto:**
  - Test signup logic: player vs waitlist routing
  - Test duplicate signup prevention
  - Test full slot/waitlist rejection

---

## Milestone 5: Cancel Booking (with Auto-Promotion)

**Goal:** User can cancel, waitlist automatically promotes.

### Tasks
- [ ] 5.1: Add "Cancel" button next to user's own booking
- [ ] 5.2: `POST /slots/{id}/cancel` - removes user's booking
- [ ] 5.3: If cancelled from player list AND waitlist not empty:
  - Promote waitlist #1 to player list
  - Shift all waitlist positions up
- [ ] 5.4: Return updated slot HTML fragment

### Test Criteria
- **Manual:**
  - Cancel from player list → waitlist #1 promoted
  - Cancel from waitlist → others shift up
  - Cancel from player list when no waitlist → just removed
- **Auto:**
  - Test cancel + promotion logic
  - Test position recalculation

---

## Milestone 6: Admin Override

**Goal:** Admin can move/remove any player.

### Tasks
- [ ] 6.1: Check `is_admin` flag from user's cookie
- [ ] 6.2: Admin sees "Remove" button on all players (not just self)
- [ ] 6.3: `POST /admin/slots/{id}/remove/{user_id}` - admin removes player
- [ ] 6.4: Admin can manually promote from waitlist (button next to waitlist names)
- [ ] 6.5: Simple admin indicator in UI (e.g., "Admin mode" badge)

### Test Criteria
- **Manual:**
  - As admin: can remove anyone, can promote anyone
  - As regular user: only see cancel for self
- **Auto:**
  - Test admin-only endpoint returns 403 for non-admin
  - Test admin removal triggers promotion

---

## Milestone 7: Auto-Create Slots

**Goal:** Slots automatically created on schedule.

### Tasks
- [ ] 7.1: Store slot schedule config (Mon 20:00 @ Venue A, Tue 20:00 @ Venue B, etc.)
- [ ] 7.2: Endpoint or cron job: create slots for next 2 weeks if not exist
- [ ] 7.3: Run on startup + can be triggered manually by admin
- [ ] 7.4: Skip slot creation for dates that already have a slot

### Test Criteria
- **Manual:**
  - Start fresh → slots for next 2 weeks appear
  - Restart → no duplicate slots created
- **Auto:**
  - Test slot generation logic
  - Test idempotency (running twice doesn't duplicate)

---

## Milestone 8: Polish & Deploy

**Goal:** Ready for real users.

### Tasks
- [ ] 8.1: Error handling: graceful errors, not panics
- [ ] 8.2: Loading states (HTMX indicators)
- [ ] 8.3: Mobile CSS polish (tap targets, readability)
- [ ] 8.4: Dockerfile for deployment
- [ ] 8.5: Deploy to Fly.io (or Hetzner)
- [ ] 8.6: Set up SQLite backup strategy

### Test Criteria
- **Manual:**
  - Full user flow works on phone
  - Admin flow works
  - Survives restart (data persists)
- **Auto:**
  - E2E test: signup → cancel → promotion flow

---

## Definition of "Minimal Deployable" (Pre-V1)

After **Milestone 5**, you have:
- ✅ View slots
- ✅ Pick your name
- ✅ Sign up / Cancel
- ✅ Automatic waitlist promotion

This is **deployable for testing** with real users (minus admin features).

---

## Testing Strategy

### Unit Tests (per milestone)
- Model/logic tests in `src/tests/`
- Database operations with in-memory SQLite

### Integration Tests
- Start actual server
- Make HTTP requests
- Assert responses

### Manual Testing Checklist
```
[ ] Mobile browser: can complete full flow
[ ] Desktop browser: same
[ ] Slow connection: HTMX loading states work
[ ] Multiple users: concurrent signups don't corrupt data
```

---

## File Structure (Final)

```
src/
  main.rs           - Server setup, routes
  models.rs         - User, Slot, Booking structs
  db.rs             - Database queries
  handlers/
    slots.rs        - Slot display, signup, cancel
    users.rs        - User selection, creation
    admin.rs        - Admin override endpoints
  middleware.rs     - Cookie/user extraction

templates/
  base.html         - Layout with HTMX
  index.html        - Home page
  slots.html        - Slot list
  slot_card.html    - Individual slot
  user_selector.html - Name dropdown

migrations/
  001_initial.sql   - Schema

tests/
  integration/      - HTTP-level tests
```

---

## Commands Reference

```bash
# Run server
cargo run

# Run tests
cargo test

# Run with auto-reload (requires cargo-watch)
cargo watch -x run

# Create migration
sqlx migrate add <name>

# Run migrations
sqlx migrate run
```
