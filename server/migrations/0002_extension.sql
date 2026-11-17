-- The RigSweep Tabs browser extension: a separate random install id, weekly counts only.
-- No URLs, titles, hosts or patterns are ever sent.

CREATE TABLE ext_installs (
    install_id    TEXT PRIMARY KEY,
    first_seen    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen     TIMESTAMPTZ NOT NULL DEFAULT now(),
    country       TEXT NOT NULL DEFAULT '??',
    ext_version   TEXT NOT NULL,
    browser       TEXT NOT NULL,
    browser_major INT NOT NULL
);

CREATE TABLE ext_reports (
    id             BIGSERIAL PRIMARY KEY,
    install_id     TEXT NOT NULL REFERENCES ext_installs ON DELETE CASCADE,
    week           DATE NOT NULL,
    received_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    country        TEXT NOT NULL,
    ext_version    TEXT NOT NULL,
    tabs_slept     BIGINT NOT NULL,
    dupes_closed   BIGINT NOT NULL,
    matches_closed BIGINT NOT NULL,
    tabs_open      INT NOT NULL,
    windows_open   INT NOT NULL,
    auto_sleep     BOOLEAN NOT NULL,
    UNIQUE (install_id, week)
);
CREATE INDEX ext_reports_week ON ext_reports (week);
