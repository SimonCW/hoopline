-- Reject enrollment requests sent before this device changed member.
ALTER TABLE push_devices ADD COLUMN identity_revision INTEGER NOT NULL DEFAULT 0;
