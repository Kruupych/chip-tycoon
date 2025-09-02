-- Add status to saves to track autosave progress
ALTER TABLE saves ADD COLUMN status TEXT NOT NULL DEFAULT 'done';

