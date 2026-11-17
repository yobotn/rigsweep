-- Store everything as it arrives: one row per report and one row per thing cleaned, nothing merged by
-- week. Totals and charts are computed at read time, and the raw rows stay available for analysis.

-- reports: no more one-row-per-week; each report keeps its own machine snapshot
ALTER TABLE reports DROP COLUMN week;                       -- also drops UNIQUE (install_id, week)
ALTER TABLE reports ALTER COLUMN processes DROP NOT NULL, ALTER COLUMN cleanups DROP NOT NULL;
ALTER TABLE reports
    ADD COLUMN v            INT NOT NULL DEFAULT 1,
    ADD COLUMN since        TIMESTAMPTZ,                     -- when the previous accepted report was sent
    ADD COLUMN windows_build TEXT,
    ADD COLUMN ram_total    BIGINT,
    ADD COLUMN ram_used_avg BIGINT,
    ADD COLUMN cores        INT;
CREATE INDEX reports_received ON reports (received_at);
CREATE INDEX reports_install ON reports (install_id, received_at);

-- events: one row per thing cleaned, as recorded on the PC (v2 reports)
CREATE TABLE events (
    id         BIGSERIAL PRIMARY KEY,
    report_id  BIGINT NOT NULL REFERENCES reports ON DELETE CASCADE,
    install_id TEXT NOT NULL REFERENCES installs ON DELETE CASCADE,
    seq        BIGINT,               -- the PC's own row number; a resent report can't add it twice
    at         TIMESTAMPTZ NOT NULL, -- when it was cleaned (to the minute)
    run        BIGINT,               -- rows of one cleanup share a run
    source     TEXT,                 -- gui | auto | watchdog | cli | import
    area       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    bytes      BIGINT NOT NULL,
    count      BIGINT NOT NULL,
    UNIQUE (install_id, seq)
);
CREATE INDEX events_at ON events (at);

-- the per-week totals of v1 reports become events dated when they arrived
INSERT INTO events (report_id, install_id, at, area, kind, bytes, count)
SELECT f.report_id, r.install_id, r.received_at, f.area, f.kind, f.bytes, f.count
FROM freed f JOIN reports r ON r.id = f.report_id;
DROP TABLE freed;

-- extension reports: already "since the last report"; now one row each instead of summed per week
ALTER TABLE ext_reports DROP COLUMN week;                   -- also drops UNIQUE (install_id, week)
ALTER TABLE ext_reports ADD COLUMN since TIMESTAMPTZ;
CREATE INDEX ext_reports_received ON ext_reports (received_at);
