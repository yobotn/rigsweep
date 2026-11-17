-- Vendor software found on a PC (services, startup entries, scheduled tasks, programs whose name or
-- vendor folder matches a keyword in discovery.json): one raw row per item per report.
CREATE TABLE seen (
    id         BIGSERIAL PRIMARY KEY,
    report_id  BIGINT NOT NULL REFERENCES reports ON DELETE CASCADE,
    install_id TEXT NOT NULL REFERENCES installs ON DELETE CASCADE,
    at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind       TEXT NOT NULL,      -- service | task | run | folder | process
    name       TEXT NOT NULL,
    display    TEXT NOT NULL,
    exe        TEXT NOT NULL,      -- file name only
    folder     TEXT NOT NULL,      -- vendor folder under Program Files / ProgramData, or ''
    keyword    TEXT NOT NULL,
    running    BOOLEAN NOT NULL,
    startup    BOOLEAN NOT NULL,
    disabled   BOOLEAN NOT NULL,   -- turned off with a RigSweep tweak
    cpu        REAL NOT NULL,
    mem        BIGINT NOT NULL
);
CREATE INDEX seen_at ON seen (at);
CREATE INDEX seen_name ON seen (lower(name));
