//! Everything that touches Postgres. Aggregates are built as JSON by Postgres itself.

use serde_json::{Map, Value};
use sqlx::PgPool;

use crate::model::{ExtReport, Report};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Store a report as it arrived: a new row every time, never merged with earlier ones. `installs`
/// only keeps the latest state of each install, for quick counts; the history is in `reports`.
pub async fn save_report(pool: &PgPool, r: &Report, country: &str) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO installs (install_id, country, app_version, windows_build, ram_total, ram_used_avg, cores)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (install_id) DO UPDATE SET last_seen = now(), country = EXCLUDED.country,
           app_version = EXCLUDED.app_version, windows_build = EXCLUDED.windows_build, ram_total = EXCLUDED.ram_total,
           ram_used_avg = EXCLUDED.ram_used_avg, cores = EXCLUDED.cores",
    )
    .bind(&r.install_id)
    .bind(country)
    .bind(&r.app_version)
    .bind(&r.windows_build)
    .bind(r.machine.ram_total as i64)
    .bind(r.machine.ram_used_avg as i64)
    .bind(r.machine.cores as i32)
    .execute(&mut *tx)
    .await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO reports (install_id, v, since, country, app_version, windows_build, ram_total, ram_used_avg, cores, processes, cleanups, tweaks)
         VALUES ($1, $2, $3::timestamptz, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id",
    )
    .bind(&r.install_id)
    .bind(r.v as i32)
    .bind(r.since.as_deref())
    .bind(country)
    .bind(&r.app_version)
    .bind(&r.windows_build)
    .bind(r.machine.ram_total as i64)
    .bind(r.machine.ram_used_avg as i64)
    .bind(r.machine.cores as i32)
    .bind(r.processes.map(|n| n as i64))
    .bind(r.cleanups.map(|n| n as i64))
    .bind(&r.tweaks)
    .fetch_one(&mut *tx)
    .await?;
    // v2: one row per thing cleaned; a resent event (same install + seq) is kept once
    sqlx::query(
        "INSERT INTO events (report_id, install_id, seq, at, run, source, area, kind, bytes, count, cpu)
         SELECT $1, $2, * FROM UNNEST($3::bigint[], $4::timestamptz[], $5::bigint[], $6::text[], $7::text[], $8::text[], $9::bigint[], $10::bigint[], $11::real[])
         ON CONFLICT (install_id, seq) DO NOTHING",
    )
    .bind(id)
    .bind(&r.install_id)
    .bind(r.events.iter().map(|e| e.seq as i64).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.at.clone()).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.run).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.source.clone()).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.area.clone()).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.kind.clone()).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.bytes as i64).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.count as i64).collect::<Vec<_>>())
    .bind(r.events.iter().map(|e| e.cpu).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;
    // The first v2 report of an install repeats, row by row, everything its v1 reports already sent as
    // totals: those v1 rows (no seq) and counts are superseded, so they go.
    if !r.events.is_empty() {
        sqlx::query("DELETE FROM events WHERE install_id = $1 AND seq IS NULL").bind(&r.install_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE reports SET processes = NULL, cleanups = NULL WHERE install_id = $1 AND v = 1")
            .bind(&r.install_id)
            .execute(&mut *tx)
            .await?;
    }
    // v1 (test builds): the 7-day totals become events dated now
    sqlx::query(
        "INSERT INTO events (report_id, install_id, at, area, kind, bytes, count)
         SELECT $1, $2, now(), * FROM UNNEST($3::text[], $4::text[], $5::bigint[], $6::bigint[])",
    )
    .bind(id)
    .bind(&r.install_id)
    .bind(r.freed.iter().map(|f| f.area.clone()).collect::<Vec<_>>())
    .bind(r.freed.iter().map(|f| f.kind.clone()).collect::<Vec<_>>())
    .bind(r.freed.iter().map(|f| f.bytes as i64).collect::<Vec<_>>())
    .bind(r.freed.iter().map(|f| f.count as i64).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;
    let apps: Vec<(&str, &crate::model::AppUse)> =
        r.top_cpu.iter().map(|a| ("cpu", a)).chain(r.top_mem.iter().map(|a| ("mem", a))).collect();
    // vendor software on the PC, one raw row per item
    let s = &r.seen;
    sqlx::query(
        "INSERT INTO seen (report_id, install_id, kind, name, display, exe, folder, keyword, running, startup, disabled, cpu, mem)
         SELECT $1, $2, * FROM UNNEST($3::text[], $4::text[], $5::text[], $6::text[], $7::text[], $8::text[], $9::bool[], $10::bool[], $11::bool[], $12::real[], $13::bigint[])",
    )
    .bind(id)
    .bind(&r.install_id)
    .bind(s.iter().map(|x| x.kind.clone()).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.name.clone()).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.display.clone()).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.exe.clone()).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.folder.clone()).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.keyword.clone()).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.running).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.startup).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.disabled).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.cpu).collect::<Vec<_>>())
    .bind(s.iter().map(|x| x.mem as i64).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO app_use (report_id, metric, app, cpu, mem) SELECT $1, * FROM UNNEST($2::text[], $3::text[], $4::real[], $5::bigint[])")
        .bind(id)
        .bind(apps.iter().map(|(m, _)| m.to_string()).collect::<Vec<_>>())
        .bind(apps.iter().map(|(_, a)| a.app.clone()).collect::<Vec<_>>())
        .bind(apps.iter().map(|(_, a)| a.cpu).collect::<Vec<_>>())
        .bind(apps.iter().map(|(_, a)| a.mem as i64).collect::<Vec<_>>())
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Store an extension report as it arrived (its counts are "since the last report"): a new row each time.
pub async fn save_ext_report(pool: &PgPool, r: &ExtReport, country: &str) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO ext_installs (install_id, country, ext_version, browser, browser_major) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (install_id) DO UPDATE SET last_seen = now(), country = EXCLUDED.country, ext_version = EXCLUDED.ext_version,
           browser = EXCLUDED.browser, browser_major = EXCLUDED.browser_major",
    )
    .bind(&r.install_id)
    .bind(country)
    .bind(&r.ext_version)
    .bind(&r.browser)
    .bind(r.browser_major as i32)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO ext_reports (install_id, since, country, ext_version, tabs_slept, dupes_closed, matches_closed, tabs_open, windows_open, auto_sleep)
         VALUES ($1, $10::timestamptz, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&r.install_id)
    .bind(country)
    .bind(&r.ext_version)
    .bind(r.tabs_slept as i64)
    .bind(r.dupes_closed as i64)
    .bind(r.matches_closed as i64)
    .bind(r.tabs_open as i32)
    .bind(r.windows_open as i32)
    .bind(r.auto_sleep)
    .bind(r.since.as_deref())
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Turning sharing off: the install and everything it sent are deleted. Returns whether it existed.
/// App and extension ids are both random, so one call covers either.
pub async fn delete_install(pool: &PgPool, id: &str) -> sqlx::Result<bool> {
    let n = sqlx::query("DELETE FROM installs WHERE install_id = $1").bind(id).execute(pool).await?.rows_affected()
        + sqlx::query("DELETE FROM ext_installs WHERE install_id = $1").bind(id).execute(pool).await?.rows_affected();
    if n > 0 {
        sqlx::query("INSERT INTO optouts (day, n) VALUES (current_date, 1) ON CONFLICT (day) DO UPDATE SET n = optouts.n + 1")
            .execute(pool)
            .await?;
    }
    Ok(n > 0)
}

async fn json(pool: &PgPool, sql: &str) -> sqlx::Result<Value> {
    // Built only from constants in this file plus `days` (a u32 we parsed) and a fixed `step`; no request
    // text ever reaches it, so it is safe to run as dynamic SQL.
    sqlx::query_scalar::<_, Value>(sqlx::AssertSqlSafe(sql.to_owned())).fetch_one(pool).await
}

/// `SELECT … ` rows as a JSON array.
async fn rows(pool: &PgPool, sql: &str) -> sqlx::Result<Value> {
    json(pool, &format!("SELECT COALESCE(json_agg(t), '[]'::json) FROM ({sql}) t")).await
}

/// Totals over `events` (and the v1 per-report counts) for the rows each condition selects.
fn sums(ev_when: &str, rep_when: &str, ext_when: &str) -> String {
    format!(
        "'disk_freed', (SELECT COALESCE(SUM(bytes), 0) FROM events WHERE area <> 'memory' AND {ev_when}),
         'ram_recovered', (SELECT COALESCE(SUM(bytes), 0) FROM events WHERE area = 'memory' AND {ev_when}),
         'processes_ended', (SELECT COALESCE(SUM(count), 0) FROM events WHERE area = 'memory' AND {ev_when}),
         'cpu_freed', (SELECT ROUND(COALESCE(SUM(cpu), 0)::numeric, 1)::float8 FROM events WHERE area = 'memory' AND {ev_when}),
         'cpu_peak', (SELECT ROUND(COALESCE(MAX(s), 0)::numeric, 1)::float8 FROM (SELECT SUM(cpu) AS s FROM events
                      WHERE area = 'memory' AND run IS NOT NULL AND {ev_when} GROUP BY install_id, run) runs),
         'cleanups', (SELECT count(DISTINCT (install_id, run)) FROM events WHERE run IS NOT NULL AND {ev_when})
                   + (SELECT COALESCE(SUM(cleanups), 0) FROM reports WHERE v = 1 AND {rep_when}),
         'tabs_slept', (SELECT COALESCE(SUM(tabs_slept), 0) FROM ext_reports WHERE {ext_when}),
         'tabs_closed', (SELECT COALESCE(SUM(dupes_closed + matches_closed), 0) FROM ext_reports WHERE {ext_when})"
    )
}

/// The public counters on rigsweep.com: all-time totals only.
pub async fn public_totals(pool: &PgPool) -> sqlx::Result<Value> {
    let mut v = json(
        pool,
        &format!(
            "SELECT json_build_object(
               'installs', (SELECT count(*) FROM installs),
               'active_installs_30d', (SELECT count(DISTINCT install_id) FROM reports WHERE received_at >= now() - interval '30 days'),
               'ext_installs', (SELECT count(*) FROM ext_installs),
               {},
               'updated_at', now())",
            sums("true", "true", "true")
        ),
    )
    .await?;
    v["top"] = rows(
        pool,
        "SELECT area, kind, SUM(bytes) AS bytes FROM events WHERE kind <> 'earlier' GROUP BY area, kind ORDER BY bytes DESC LIMIT 10",
    )
    .await?;
    Ok(v)
}

/// Everything on the owner's dashboard for the last `days` days (None = all time), computed from the
/// raw rows at read time. `prev` holds the same totals for the period before, for the change arrows.
pub async fn admin_summary(pool: &PgPool, days: Option<u32>) -> sqlx::Result<Value> {
    // `days` is a number parsed by us, so formatting it into SQL is safe
    let (cur, prev) = match days {
        Some(d) => (format!("now() - interval '{d} days'"), format!("now() - interval '{} days'", d * 2)),
        None => ("'-infinity'::timestamptz".to_string(), "'-infinity'::timestamptz".to_string()),
    };
    let span = |col: &str, from: &str, to: &str| format!("{col} >= {from} AND {col} < {to}");
    let period = |from: &str, to: &str| {
        format!(
            "json_build_object(
               'active', (SELECT count(DISTINCT install_id) FROM reports WHERE {r}),
               'new_installs', (SELECT count(*) FROM installs WHERE {f}),
               'reports', (SELECT count(*) FROM reports WHERE {r}),
               'optouts', (SELECT COALESCE(SUM(n), 0) FROM optouts WHERE {o}),
               'ext_active', (SELECT count(DISTINCT install_id) FROM ext_reports WHERE {r}),
               {s})",
            r = span("received_at", from, to),
            f = span("first_seen", from, to),
            o = format!("day >= ({from})::date AND day < ({to})::date + 1"),
            s = sums(&span("at", from, to), &span("received_at", from, to), &span("received_at", from, to)),
        )
    };
    let mut m = Map::new();
    m.insert("days".into(), days.map(Value::from).unwrap_or(Value::Null));
    m.insert(
        "totals".into(),
        json(
            pool,
            &format!(
                "SELECT json_build_object(
                   'now', {now},
                   'prev', {prev},
                   'all', json_build_object('installs', (SELECT count(*) FROM installs), 'ext_installs', (SELECT count(*) FROM ext_installs),
                                            'reports', (SELECT count(*) FROM reports), 'events', (SELECT count(*) FROM events)),
                   'ram_used_pct', (SELECT round(avg(ram_used_avg::numeric * 100 / NULLIF(ram_total, 0)), 1) FROM reports WHERE received_at >= {cur}),
                   'ext_tabs_avg', (SELECT round(avg(tabs_open), 0) FROM ext_reports WHERE received_at >= {cur}),
                   'ext_auto_sleep_pct', (SELECT round(100.0 * count(*) FILTER (WHERE auto_sleep) / NULLIF(count(*), 0), 0) FROM ext_reports
                                          WHERE received_at >= {cur}))",
                now = period(&cur, "now() + interval '1 day'"),
                prev = if days.is_some() { period(&prev, &cur) } else { "NULL".into() },
            ),
        )
        .await?,
    );
    // per hour for 1 day, per day otherwise, with zeros where nothing happened
    let step = if days == Some(1) { "hour" } else { "day" };
    let first = match days {
        Some(_) => format!("date_trunc('{step}', {cur})"),
        None => format!("date_trunc('{step}', COALESCE((SELECT min(received_at) FROM reports), now()))"),
    };
    let fleet = latest(&cur);
    let sections: Vec<(&str, String)> = vec![
        ("series", format!(
            "SELECT b.t AS day, COALESCE(r.active, 0) AS active, COALESCE(r.reports, 0) AS reports,
                    COALESCE(e.disk, 0) AS disk, COALESCE(e.ram, 0) AS ram, COALESCE(x.active, 0) AS ext_active
             FROM generate_series({first}, date_trunc('{step}', now()), interval '1 {step}') AS b(t)
             LEFT JOIN (SELECT date_trunc('{step}', received_at) t, count(DISTINCT install_id) active, count(*) reports FROM reports GROUP BY 1) r ON r.t = b.t
             LEFT JOIN (SELECT date_trunc('{step}', at) t, SUM(bytes) FILTER (WHERE area <> 'memory') disk,
                               SUM(bytes) FILTER (WHERE area = 'memory') ram FROM events GROUP BY 1) e ON e.t = b.t
             LEFT JOIN (SELECT date_trunc('{step}', received_at) t, count(DISTINCT install_id) active FROM ext_reports GROUP BY 1) x ON x.t = b.t
             ORDER BY b.t")),
        ("freed", format!(
            "SELECT area, kind, SUM(bytes) AS bytes, SUM(count) AS count, count(DISTINCT install_id) AS installs
             FROM events WHERE at >= {cur} GROUP BY area, kind ORDER BY bytes DESC LIMIT 40")),
        ("sources", format!(
            "SELECT COALESCE(source, 'earlier') AS name, count(DISTINCT (install_id, run)) AS n, SUM(bytes) AS bytes
             FROM events WHERE at >= {cur} GROUP BY 1 ORDER BY bytes DESC")),
        ("countries", format!(
            "SELECT country, count(DISTINCT install_id) AS n FROM reports WHERE received_at >= {cur}
             GROUP BY country ORDER BY n DESC, country LIMIT 40")),
        ("apps_cpu", format!(
            "SELECT a.app, round(avg(a.cpu)::numeric, 1) AS avg_cpu, count(DISTINCT r.install_id) AS installs
             FROM app_use a JOIN reports r ON r.id = a.report_id WHERE a.metric = 'cpu' AND r.received_at >= {cur}
             GROUP BY a.app ORDER BY installs DESC, avg_cpu DESC LIMIT 20")),
        ("apps_mem", format!(
            "SELECT a.app, avg(a.mem)::bigint AS avg_mem, count(DISTINCT r.install_id) AS installs
             FROM app_use a JOIN reports r ON r.id = a.report_id WHERE a.metric = 'mem' AND r.received_at >= {cur}
             GROUP BY a.app ORDER BY installs DESC, avg_mem DESC LIMIT 20")),
        ("versions", format!("SELECT app_version AS name, count(*) AS n FROM {fleet} GROUP BY 1 ORDER BY n DESC LIMIT 15")),
        ("builds", format!("SELECT windows_build AS name, count(*) AS n FROM {fleet} GROUP BY 1 ORDER BY n DESC LIMIT 15")),
        ("ram", format!(
            "SELECT CASE WHEN ram_total <= 9 * 1024^3 THEN '≤ 8 GB' WHEN ram_total <= 17 * 1024^3 THEN '16 GB'
                    WHEN ram_total <= 33 * 1024^3 THEN '32 GB' WHEN ram_total <= 65 * 1024^3 THEN '64 GB' ELSE '> 64 GB' END AS name,
                    count(*) AS n, min(ram_total) AS lo
             FROM {fleet} GROUP BY 1 ORDER BY lo")),
        ("cores", format!("SELECT cores AS name, count(*) AS n FROM {fleet} GROUP BY cores ORDER BY cores")),
        ("tweaks", format!("SELECT t AS tweak, count(*) AS installs FROM {fleet}, unnest(latest.tweaks) AS t GROUP BY t ORDER BY installs DESC, t")),
        ("ext_browsers", format!("SELECT browser AS name, count(*) AS n FROM ext_installs WHERE last_seen >= {cur} GROUP BY 1 ORDER BY n DESC")),
        // vendor software across installs: the evidence for which tweaks to build next
        ("discovered", format!(
            "SELECT kind, name, exe, max(folder) AS folder, keyword, count(DISTINCT install_id) AS installs,
                    round(100.0 * avg(running::int)) AS running_pct, round(100.0 * avg(startup::int)) AS startup_pct,
                    round(avg(cpu)::numeric, 1) AS avg_cpu, avg(mem)::bigint AS avg_mem,
                    count(DISTINCT install_id) FILTER (WHERE disabled) AS disabled
             FROM seen WHERE at >= {cur} GROUP BY kind, name, exe, keyword
             ORDER BY installs DESC, avg_mem DESC, name LIMIT 300")),
        ("ext_tabs", format!(
            "SELECT CASE WHEN tabs_open <= 10 THEN '≤ 10' WHEN tabs_open <= 30 THEN '11–30' WHEN tabs_open <= 60 THEN '31–60'
                    WHEN tabs_open <= 100 THEN '61–100' ELSE '> 100' END AS name, count(*) AS n, min(tabs_open) AS lo
             FROM (SELECT DISTINCT ON (install_id) install_id, tabs_open FROM ext_reports
                   WHERE received_at >= {cur} ORDER BY install_id, received_at DESC) x
             GROUP BY 1 ORDER BY lo")),
    ];
    for (name, sql) in sections {
        m.insert(name.into(), rows(pool, &sql).await?);
    }
    Ok(Value::Object(m))
}

/// The newest report of every install heard from since `from` (v1 rows have no machine columns).
fn latest(from: &str) -> String {
    format!(
        "(SELECT DISTINCT ON (install_id) * FROM reports WHERE received_at >= {from} AND ram_total IS NOT NULL
          ORDER BY install_id, received_at DESC) latest"
    )
}

/// Raw tables the owner can download as CSV (for a spreadsheet or a BI tool).
pub const EXPORTS: &[&str] = &["installs", "reports", "events", "app_use", "seen", "ext_installs", "ext_reports", "optouts"];

/// One raw table as CSV with a header row, oldest first. `table` must be one of `EXPORTS`.
pub async fn export_csv(pool: &PgPool, table: &str) -> sqlx::Result<Vec<u8>> {
    use futures_util::TryStreamExt;
    use sqlx::postgres::PgPoolCopyExt;
    if !EXPORTS.contains(&table) {
        return Err(sqlx::Error::RowNotFound);
    }
    let order = match table {
        "installs" | "ext_installs" => "first_seen",
        "optouts" => "day",
        "app_use" => "report_id",
        _ => "id",
    };
    let stream = pool.copy_out_raw(&format!("COPY (SELECT * FROM {table} ORDER BY {order}) TO STDOUT WITH (FORMAT csv, HEADER true)")).await?;
    let chunks: Vec<_> = stream.try_collect().await?;
    Ok(chunks.concat())
}
