# Basketball Slot Booking - Problem Description

## Context

A hobby basketball group of **61 members** organizes pickup games with:
- **3 fixed weekly slots** at different venues:
  - Mondays 20:00–22:00
  - Tuesdays 20:00–21:45
  - Thursdays 20:00–22:00
- **15 player spots** per slot
- **5-person waitlist** per slot
- **2-week advance booking window** (new slots open Sunday evenings)

Currently managed via a shared **Google Spreadsheet** with columns for each date/venue.

---

## Current Problems

### 1. Waitlist Queue Jumping (Most Common Conflict)
**The Problem:** When a player cancels and the #1 waitlist person gets promoted, that cell becomes empty. Some people then write their name directly into this "top spot" instead of adding themselves at the bottom of the waitlist and shifting everyone up.

**Root Cause:** 
- No enforcement mechanism - anyone can edit any cell
- People either don't understand the queue system or exploit it
- No clear visual indication of where to add your name

**Impact:** Unfair to people who followed the rules; causes disputes

---

### 2. Anonymous Edits Make Disputes Unresolvable
**The Problem:** Most users access the spreadsheet anonymously (not logged into Google). When someone's name gets deleted—whether accidentally or intentionally—there's no way to know who did it or restore the state.

**Root Cause:**
- Google Sheets edit history only shows "Anonymous" for most users
- No audit trail of who made what change

**Impact:** 
- Disputes about "I was signed up, someone deleted me"
- Requires weekly admin intervention to resolve conflicts
- Time-consuming edit history forensics with no clear answers

---

### 3. Manual Slot Creation
**The Problem:** Admins must manually create new slot columns each week (every Sunday evening).

**Root Cause:** No automation - spreadsheet needs manual updates

**Impact:** Administrative overhead; if admin forgets, people can't sign up

---

### 4. Waitlist Promotion is Manual and Error-Prone
**The Problem:** When someone cancels, the cancelling player or waitlist #1 is supposed to:
1. Remove the player from the list
2. Move all waitlist players up one position
3. Notify the promoted person via WhatsApp

**Root Cause:** 
- Multi-step manual process with no automation
- No built-in notification system

**Impact:**
- Steps often skipped or done incorrectly
- People on waitlist don't find out they got a spot in time
- The promoted person may not have time to prepare/show up

---

### 5. Name ↔ Contact Mapping is Difficult
**The Problem:** When notifying someone via WhatsApp, it's hard to know which WhatsApp contact corresponds to which spreadsheet name.

**Root Cause:**
- Spreadsheet only has first names (often nicknames)
- No phone numbers or contact info linked to names
- WhatsApp display names may differ from spreadsheet names

**Impact:** Delays in notification; sometimes wrong person contacted

---

### 6. No Cancellation Etiquette Enforcement
**The Problem:** There's an unwritten rule that cancellations should happen with enough notice (ideally by 2pm same day) to give waitlisted players time to adjust plans.

**Root Cause:** 
- No formal deadline
- No tracking of late cancellations

**Impact:** 
- Waitlist players sometimes find out too late to participate
- Social friction from last-minute cancellations

---

### 7. No-Show Tracking is Informal
**The Problem:** Some players sign up but don't show up and don't cancel. This is tracked "in people's heads" rather than systematically.

**Root Cause:** No attendance tracking mechanism

**Impact:**
- Wasted spots that could have gone to waitlisted players
- No consequences for repeat offenders
- Resentment among reliable players

---

## Stakeholders

| Role | Count | Responsibilities |
|------|-------|------------------|
| **Regular Members** | ~61 | Sign up for slots, manage their own bookings, cancel if needed |
| **Admins** | Few (2) | Create new slots, resolve disputes, moderate |

---

## Current Workflow

```
SUNDAY EVENING:
  Admin creates new slot columns for 2 weeks out

THROUGHOUT THE WEEK:
  Players add their name to slot (player list or waitlist)
  
WHEN SOMEONE CANCELS:
  1. Cancelling player removes their name
  2. Cancelling player (ideally) messages waitlist #1 on WhatsApp
  3. Waitlist #1 moves to player list
  4. Everyone on waitlist should shift up (often forgotten)
  
WHEN DISPUTES ARISE:
  Admin checks edit history (mostly unhelpful due to anonymous edits)
  Admin makes judgment call
```

---

## Key Requirements (from problem analysis)

1. **Enforce waitlist order** - No queue jumping
2. **Track who made changes** - Accountability & audit trail
3. **Automate slot creation** - Recurring schedule
4. **Automate waitlist promotion** - When someone cancels, automatically promote next person
5. **Mobile-friendly** - Most users will access from phones

## Optional Requirements

6. **Notifications** - Tell people when they get a spot
7. **Identity management** - Link names to contact info

---

## Open Questions for Solution Design

1. **Authentication approach:** 
   - Full accounts with email/password?
   - Magic links via email?
   - Phone number verification?
   - Simple PIN per user?

2. **Hosting/cost constraints:**
   - Budget for hosting?
   - Acceptable recurring costs?
   - Self-hosted vs. managed services?

3. **WhatsApp integration:**
   - Is automated WhatsApp notification desired/feasible?
   - Or fallback to email/SMS/push notifications?

4. **Migration strategy:**
   - Import existing member list from spreadsheet?
   - Run parallel systems during transition?

5. **Fairness features (future):**
   - Should there be limits on bookings per week?
   - Priority for people who haven't played recently?
   - Penalties for no-shows?

---

## Summary of Pain Points (Ranked)

| Priority | Problem | Severity | Frequency |
|----------|---------|----------|-----------|
| 1 | Waitlist queue jumping | High | Weekly |
| 2 | Anonymous edits / no accountability | High | Weekly |
| 3 | Manual waitlist promotion | Medium | Multiple times/week |
| 4 | Name ↔ contact confusion | Medium | Multiple times/week |
| 5 | Manual slot creation | Low | Weekly |
| 6 | No-show tracking | Low | Occasional |
| 7 | Late cancellation handling | Low | Occasional |

---

*This document describes the problem space only. Solution design will follow in a separate document.*
