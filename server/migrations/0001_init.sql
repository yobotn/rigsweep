-- RigSweep anonymous usage totals. No IP addresses, paths, names or hardware IDs are stored:
-- an install is a random id the app made, and a country code looked up when a report arrives.

CREATE TABLE installs (
    install_id    TEXT PRIMARY KEY,
    first_seen    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen     TIMESTAMPTZ NOT NULL DEFAULT now(),
    country       TEXT NOT NULL DEFAULT '??',
    app_version   TEXT NOT NULL,
    windows_build TEXT NOT NULL,
    ram_total     BIGINT NOT NULL,
    ram_used_avg  BIGINT NOT NULL,
    cores         INT NOT NULL
);

-- one row per install per week; a second report in the same week replaces the first
CREATE TABLE reports (
    id          BIGSERIAL PRIMARY KEY,
    install_id  TEXT NOT NULL REFERENCES installs ON DELETE CASCADE,
    week        DATE NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    country     TEXT NOT NULL,
    app_version TEXT NOT NULL,
    processes   BIGINT NOT NULL,
    cleanups    BIGINT NOT NULL,
    tweaks      TEXT[] NOT NULL DEFAULT '{}',
    UNIQUE (install_id, week)
);
CREATE INDEX reports_week ON reports (week);

CREATE TABLE freed (
    report_id BIGINT NOT NULL REFERENCES reports ON DELETE CASCADE,
    area      TEXT NOT NULL,
    kind      TEXT NOT NULL,
    bytes     BIGINT NOT NULL,
    count     BIGINT NOT NULL
);
CREATE INDEX freed_report ON freed (report_id);

CREATE TABLE app_use (
    report_id BIGINT NOT NULL REFERENCES reports ON DELETE CASCADE,
    metric    TEXT NOT NULL CHECK (metric IN ('cpu', 'mem')),
    app       TEXT NOT NULL,
    cpu       REAL NOT NULL,
    mem       BIGINT NOT NULL
);
CREATE INDEX app_use_report ON app_use (report_id);

-- turning sharing off deletes the install's data; only a daily count of that is kept
CREATE TABLE optouts (
    day DATE PRIMARY KEY,
    n   INT NOT NULL
);
