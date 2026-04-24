-- Add color column to environments table
ALTER TABLE environments
    ADD COLUMN color TEXT NOT NULL DEFAULT 'green'
    CONSTRAINT valid_environment_color CHECK (color IN ('green', 'blue', 'yellow', 'red', 'purple', 'orange', 'gray'));
