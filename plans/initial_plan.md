# Initial Plan - Basketball Slot Booking App

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
- [ ] View upcoming slots (next 2 weeks)
- [ ] See who's signed up + waitlist for each slot
- [ ] Pick your name from dropdown (remembered via cookie)
- [ ] Sign up for a slot (goes to waitlist if full)
- [ ] Cancel your booking (auto-promotes waitlist #1)

**Admin-facing:**
- [ ] Slots auto-create on schedule (e.g., every Sunday for 2 weeks out)
- [ ] Manual override: move/remove players, promote from waitlist
- [ ] Simple admin UI (no SQL knowledge needed)

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

1. Decide on hosting approach
2. Set up SQLite schema
3. Build the slot list view
4. Add sign-up / cancel with waitlist logic
5. Add admin override UI
6. Deploy and test with a few people
