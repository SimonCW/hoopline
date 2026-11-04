ALTER TABLE slots ADD COLUMN schedule_id INTEGER REFERENCES slot_schedules(id);
ALTER TABLE slots ADD COLUMN occurrence_date TEXT;
ALTER TABLE slots ADD COLUMN is_override INTEGER NOT NULL DEFAULT 0 CHECK (is_override IN (0, 1));
CREATE UNIQUE INDEX slots_schedule_occurrence_unique ON slots(schedule_id, occurrence_date);
CREATE TABLE slot_generation_exclusions (
    venue TEXT NOT NULL,
    local_date TEXT NOT NULL,
    PRIMARY KEY (venue, local_date)
);
