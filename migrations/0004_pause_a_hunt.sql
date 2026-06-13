ALTER TABLE hunts
  DROP CONSTRAINT hunts_status_check,
  ADD CONSTRAINT hunts_status_check
    CHECK (status IN ('active', 'draft', 'archived', 'paused'));
