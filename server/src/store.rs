//! Everything that touches Postgres. Aggregates are built as JSON by Postgres itself.

use serde_json::{Map, Value};
use sqlx::PgPool;

use crate::model::{ExtReport, Report};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Store a report; a second one from the same install in the same week replaces the first.
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
        "INSERT INTO reports (install_id, week, country, app_version, processes, cleanups, tweaks)
         VALUES ($1, date_trunc('week', now())::date, $2, $3, $4, $5, $6)
         ON CONFLICT (install_id, week) DO UPDATE SET received_at = now(), country = EXCLUDED.country,
           app_version = EXCLUDED.app_version, processes = EXCLUDED.processes, cleanups = EXCLUDED.cleanups,
           tweaks = EXCLUDED.tweaks
         RETURNING id",
    )
    .bind(&r.install_id)
    .bind(country)
    .bind(&r.app_version)
    .bind(r.processes as i64)
    .bind(r.cleanups as i64)
    .bind(&r.tweaks)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM freed WHERE report_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM app_use WHERE report_id = $1").bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO freed (report_id, area, kind, bytes, count) SELECT $1, * FROM UNNEST($2::text[], $3::text[], $4::bigint[], $5::bigint[])")
        .bind(id)
        .bind(r.freed.iter().map(|f| f.area.clone()).collect::<Vec<_>>())
        .bind(r.freed.iter().map(|f| f.kind.clone()).collect::<Vec<_>>())
        .bind(r.freed.iter().map(|f| f.bytes as i64).collect::<Vec<_>>())
        .bind(r.freed.iter().map(|f| f.count as i64).collect::<Vec<_>>())
        .execute(&mut *tx)
        .await?;
    let apps: Vec<(&str, &crate::model::AppUse)> =
        r.top_cpu.iter().map(|a| ("cpu", a)).chain(r.top_mem.iter().map(|a| ("mem", a))).collect();
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

/// Store an extension report. Its counts are "since the last report", so two in one week add up.
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
        "INSERT INTO ext_reports (install_id, week, country, ext_version, tabs_slept, dupes_closed, matches_closed, tabs_open, windows_open, auto_sleep)
         VALUES ($1, date_trunc('week', now())::date, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (install_id, week) DO UPDATE SET received_at = now(), country = EXCLUDED.country, ext_version = EXCLUDED.ext_version,
           tabs_slept = ext_reports.tabs_slept + EXCLUDED.tabs_slept, dupes_closed = ext_reports.dupes_closed + EXCLUDED.dupes_closed,
           matches_closed = ext_reports.matches_closed + EXCLUDED.matches_closed, tabs_open = EXCLUDED.tabs_open,
           windows_open = EXCLUDED.windows_open, auto_sleep = EXCLUDED.auto_sleep",
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
    sqlx::query_scalar::<_, Value>(sql).fetch_one(pool).await
}

/// `SELECT … ` rows as a JSON array.
async fn rows(pool: &PgPool, sql: &str) -> sqlx::Result<Value> {
    json(pool, &format!("SELECT COALESCE(json_agg(t), '[]'::json) FROM ({sql}) t")).await
}

const SUMS: &str = "'disk_freed', (SELECT COALESCE(SUM(bytes) FILTER (WHERE area <> 'memory'), 0) FROM freed),
    'ram_recovered', (SELECT COALESCE(SUM(bytes) FILTER (WHERE area = 'memory'), 0) FROM freed),
    'processes_ended', (SELECT COALESCE(SUM(processes), 0) FROM reports),
    'cleanups', (SELECT COALESCE(SUM(cleanups), 0) FROM reports),
    'ext_installs', (SELECT count(*) FROM ext_installs),
    'tabs_slept', (SELECT COALESCE(SUM(tabs_slept), 0) FROM ext_reports),
    'tabs_closed', (SELECT COALESCE(SUM(dupes_closed + matches_closed), 0) FROM ext_reports)";

/// The public counters on rigsweep.com: totals only.
pub async fn public_totals(pool: &PgPool) -> sqlx::Result<Value> {
    let mut v = json(
        pool,
        &format!(
            "SELECT json_build_object(
               'installs', (SELECT count(*) FROM installs),
               'active_installs_30d', (SELECT count(DISTINCT install_id) FROM reports WHERE week >= (now() - interval '30 days')::date),
               {SUMS},
               'updated_at', now())"
        ),
    )
    .await?;
    v["top"] = rows(
        pool,
        "SELECT area, kind, SUM(bytes) AS bytes FROM freed WHERE kind <> 'earlier' GROUP BY area, kind ORDER BY bytes DESC LIMIT 10",
    )
    .await?;
    Ok(v)
}

const LAST_30: &str = "r.week >= (now() - interval '30 days')::date";

/// Everything on the owner's dashboard.
pub async fn admin_summary(pool: &PgPool) -> sqlx::Result<Value> {
    let mut m = Map::new();
    m.insert(
        "totals".into(),
        json(
            pool,
            &format!(
                "SELECT json_build_object(
                   'installs', (SELECT count(*) FROM installs),
                   'active_7d', (SELECT count(DISTINCT install_id) FROM reports WHERE week >= (now() - interval '7 days')::date),
                   'active_30d', (SELECT count(DISTINCT install_id) FROM reports WHERE week >= (now() - interval '30 days')::date),
                   'reports', (SELECT count(*) FROM reports),
                   'optouts_30d', (SELECT COALESCE(SUM(n), 0) FROM optouts WHERE day >= (now() - interval '30 days')::date),
                   'ext_active_30d', (SELECT count(DISTINCT install_id) FROM ext_reports WHERE week >= (now() - interval '30 days')::date),
                   'ext_tabs_avg', (SELECT round(avg(tabs_open), 0) FROM ext_reports WHERE week >= (now() - interval '30 days')::date),
                   'ext_auto_sleep_pct', (SELECT round(100.0 * count(*) FILTER (WHERE auto_sleep) / NULLIF(count(*), 0), 0) FROM ext_reports
                                          WHERE week >= (now() - interval '30 days')::date),
                   'ram_used_pct', (SELECT round(avg(ram_used_avg::numeric * 100 / NULLIF(ram_total, 0)), 1) FROM installs
                                    WHERE last_seen >= now() - interval '30 days'),
                   {SUMS})"
            ),
        )
        .await?,
    );
    let sections: &[(&str, String)] = &[
        ("weekly", "SELECT r.week, count(DISTINCT r.install_id) AS active, COALESCE(SUM(f.bytes) FILTER (WHERE f.area <> 'memory'), 0) AS disk,
                    COALESCE(SUM(f.bytes) FILTER (WHERE f.area = 'memory'), 0) AS ram
                    FROM reports r LEFT JOIN freed f ON f.report_id = r.id
                    WHERE r.week >= (now() - interval '12 weeks')::date GROUP BY r.week ORDER BY r.week".into()),
        ("new_installs", "SELECT date_trunc('week', first_seen)::date AS week, count(*) AS n FROM installs
                          WHERE first_seen >= now() - interval '12 weeks' GROUP BY 1 ORDER BY 1".into()),
        ("countries", "SELECT country, count(*) AS n FROM installs WHERE last_seen >= now() - interval '30 days'
                       GROUP BY country ORDER BY n DESC, country LIMIT 40".into()),
        ("versions", "SELECT app_version AS name, count(*) AS n FROM installs WHERE last_seen >= now() - interval '30 days'
                      GROUP BY 1 ORDER BY n DESC LIMIT 15".into()),
        ("builds", "SELECT windows_build AS name, count(*) AS n FROM installs WHERE last_seen >= now() - interval '30 days'
                    GROUP BY 1 ORDER BY n DESC LIMIT 15".into()),
        ("freed", format!("SELECT f.area, f.kind, SUM(f.bytes) AS bytes, SUM(f.count) AS count, count(DISTINCT r.install_id) AS installs
                   FROM freed f JOIN reports r ON r.id = f.report_id WHERE {LAST_30}
                   GROUP BY f.area, f.kind ORDER BY bytes DESC LIMIT 40")),
        ("apps_cpu", format!("SELECT a.app, round(avg(a.cpu)::numeric, 1) AS avg_cpu, count(DISTINCT r.install_id) AS installs
                      FROM app_use a JOIN reports r ON r.id = a.report_id WHERE a.metric = 'cpu' AND {LAST_30}
                      GROUP BY a.app ORDER BY installs DESC, avg_cpu DESC LIMIT 20")),
        ("apps_mem", format!("SELECT a.app, avg(a.mem)::bigint AS avg_mem, count(DISTINCT r.install_id) AS installs
                      FROM app_use a JOIN reports r ON r.id = a.report_id WHERE a.metric = 'mem' AND {LAST_30}
                      GROUP BY a.app ORDER BY installs DESC, avg_mem DESC LIMIT 20")),
        ("ram", "SELECT CASE WHEN ram_total <= 9 * 1024^3 THEN '≤ 8 GB' WHEN ram_total <= 17 * 1024^3 THEN '16 GB'
                   WHEN ram_total <= 33 * 1024^3 THEN '32 GB' WHEN ram_total <= 65 * 1024^3 THEN '64 GB' ELSE '> 64 GB' END AS name,
                   count(*) AS n, min(ram_total) AS lo
                 FROM installs WHERE last_seen >= now() - interval '30 days' GROUP BY 1 ORDER BY lo".into()),
        ("cores", "SELECT cores AS name, count(*) AS n FROM installs WHERE last_seen >= now() - interval '30 days'
                   GROUP BY cores ORDER BY cores".into()),
        ("ext_weekly", "SELECT week, count(DISTINCT install_id) AS active, SUM(tabs_slept) AS slept, SUM(dupes_closed + matches_closed) AS closed
                        FROM ext_reports WHERE week >= (now() - interval '12 weeks')::date GROUP BY week ORDER BY week".into()),
        ("ext_browsers", "SELECT browser AS name, count(*) AS n FROM ext_installs WHERE last_seen >= now() - interval '30 days'
                          GROUP BY 1 ORDER BY n DESC".into()),
        ("ext_tabs", "SELECT CASE WHEN tabs_open <= 10 THEN '≤ 10' WHEN tabs_open <= 30 THEN '11–30' WHEN tabs_open <= 60 THEN '31–60'
                        WHEN tabs_open <= 100 THEN '61–100' ELSE '> 100' END AS name, count(*) AS n, min(tabs_open) AS lo
                      FROM (SELECT DISTINCT ON (install_id) install_id, tabs_open FROM ext_reports
                            WHERE week >= (now() - interval '30 days')::date ORDER BY install_id, week DESC) x
                      GROUP BY 1 ORDER BY lo".into()),
        ("tweaks", format!("SELECT t AS tweak, count(DISTINCT r.install_id) AS installs FROM reports r, unnest(r.tweaks) AS t
                    WHERE {LAST_30} GROUP BY t ORDER BY installs DESC, t LIMIT 25")),
    ];
    for (name, sql) in sections {
        m.insert((*name).into(), rows(pool, sql).await?);
    }
    Ok(Value::Object(m))
}
