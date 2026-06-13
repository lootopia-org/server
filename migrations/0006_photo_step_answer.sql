-- Photo step reference images are stored as base64 in awnser; VARCHAR(50) is too small.
ALTER TABLE hunt_steps ALTER COLUMN awnser TYPE TEXT;
