ALTER TABLE time_off ADD COLUMN label TEXT;

CREATE TABLE IF NOT EXISTS config (
    id                    INTEGER NOT NULL CHECK (id = 1) PRIMARY KEY,
    hours_per_holiday     INTEGER NOT NULL,
    expected_weekly_hours INTEGER NOT NULL
);

INSERT INTO config (id, hours_per_holiday, expected_weekly_hours)
VALUES (1, 8, 40);
