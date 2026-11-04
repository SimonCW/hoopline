# Initial Plan - Basketball Slot Booking App

This is the original scope. For current priorities and open tasks, see `refinements.md`.

## Problem Summary

Replace a Google Spreadsheet for booking basketball slots (61 members, 3 weekly slots, 15 players + 5 waitlist each). Main pain points: manual waitlist promotion and admin overhead.

---

## V1 Scope (MVP)

### Goals
1. **Automatic waitlist promotion** - When someone cancels, queue shuffles automatically
2. **Reduce admin overhead** - Slots auto-create on recurring schedule
3. **Keep it simple** - No worse UX than the spreadsheet

### Tech Stack
- **Backend:** Rust + Axum
- **Frontend:** HTMX + Askama templates
- **Database:** SQLite (single file, easy backup)
- **Auth:** None—pick your name from dropdown, stored in cookie

### Security Model
- **Same as current spreadsheet:** security through obscurity
- Obscure URL (no bit.ly—just share the direct link privately)
- `robots.txt` with `Disallow: /` to discourage crawlers
- No passwords, no PINs—if someone knows the URL, they can use it

### Features

**User-facing:**
- [x] View upcoming slots (currently a rolling 14-day window; Sunday release is planned)
- [x] See who's signed up + waitlist for each slot
- [x] Pick your name from dropdown (remembered via cookie)
- [x] Sign up for a slot (goes to waitlist if full)
- [x] Cancel your booking (auto-promotes waitlist #1)

**Admin-facing:**
- [x] Slots auto-create on a rolling schedule.
- [ ] Switch generation to the agreed Sunday 19:00 Berlin release. Sunday 8 November 2026 opens the week of 16–22 November. Keep already-created games and bookings.
- [ ] Manual override: removal and promotion exist; arbitrary reassignment remains open.
- [ ] Simple admin UI: existing controls support removal and promotion; full editing remains open.

### Data Model
```
User: id, name, is_admin
Slot: id, datetime, venue, max_players (15), max_waitlist (5)
Booking: id, slot_id, user_id, position, is_waitlist, created_at
```

### What's Explicitly OUT of V1
- Notifications (continue manual WhatsApp for now)
- No-show tracking
- Late cancellation penalties
- Audit log / edit history
- Per-user booking limits

---

## V2 Ideas (Future)

- **Notifications:** Email or web push when promoted from waitlist
- **Audit log:** Track who made what changes (solves accountability problem)
- **No-show tracking:** Mark attendance, flag repeat offenders
- **Cancellation deadlines:** Warn about late cancellations
- **Booking limits:** Max slots per person per week
- **Actual auth:** Add PINs or magic links if impersonation becomes a problem

---

## Decisions

1. **Hosting:** Fly.io (free tier). Later migrate to Hetzner VPS when comfortable with server admin.
2. **Member list:** Self-registration. Autocomplete input for existing users, type new name + confirm to create new user.
3. **Slot schedule:** Configurable via admin UI. Default recurring schedule (Mon/Tue/Thu) with ability to skip, add, or modify individual slots for holidays etc.

---

## Next Steps

1. Build the admin experience using `admin_experience.md` in three increments: mode/rosters, individual game edits, then recurring defaults and Sunday release. Keep authentication changes out of scope.
2. Make backups safe and verify a restore and persistent deployment storage.
3. Confirm the no-auth policy and separate sample data before a user trial.
4. Test with members on real devices and resolve the open schedule and language decisions.
