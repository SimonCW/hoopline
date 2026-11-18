-- Existing devices have no saved language until the next enrollment or switch.
ALTER TABLE push_devices ADD COLUMN locale TEXT CHECK (locale IN ('en', 'de'));
