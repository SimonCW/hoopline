-- Schedule times now refer to Europe/Berlin, not UTC.
-- Existing slots retain their booked instant. Generation must not duplicate them.
ALTER TABLE slot_schedules RENAME COLUMN time_utc TO time_local;
