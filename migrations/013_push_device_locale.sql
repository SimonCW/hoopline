ALTER TABLE push_devices ADD COLUMN locale TEXT NOT NULL DEFAULT 'en'
    CHECK (locale IN ('en', 'de'));
