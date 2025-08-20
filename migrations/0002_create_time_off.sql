CREATE TABLE IF NOT EXISTS time_off (
    id INTEGER  NOT NULL PRIMARY KEY ,
    date DATE   NOT NULL UNIQUE,
    kind TEXT   NOT NULL CHECK (kind IN ('holiday', 'vacation'))
);
