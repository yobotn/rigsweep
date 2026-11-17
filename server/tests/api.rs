//! End to end against a real PostgreSQL, downloaded and started by postgresql_embedded (no Docker).

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use rigsweep_stats::{router, Config, Geo, MIGRATOR};

fn report(id: &str, target_bytes: u64) -> Value {
    json!({
        "v": 1, "install_id": id, "app_version": "0.8.0", "windows_build": "26300",
        "processes": 3, "cleanups": 2,
        "freed": [{"area": "disk", "kind": "dir:target", "bytes": target_bytes, "count": 1},
                  {"area": "memory", "kind": "mcp", "bytes": 500, "count": 2}],
        "machine": {"ram_total": 34359738368u64, "ram_used_avg": 17179869184u64, "cores": 16},
        "top_cpu": [{"app": "VS Code", "cpu": 12.5, "mem": 900}],
        "top_mem": [{"app": "Chrome", "cpu": 3.0, "mem": 4000}],
        "tweaks": ["telemetry.diagtrack"]
    })
}

async fn call(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn post(v: &Value, ip: &str) -> Request<Body> {
    Request::post("/v1/report")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-real-ip", ip)
        .body(Body::from(v.to_string()))
        .unwrap()
}

fn get(path: &str, auth: Option<&str>) -> Request<Body> {
    let mut r = Request::get(path);
    if let Some(a) = auth {
        use base64::Engine;
        r = r.header(header::AUTHORIZATION, format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(a)));
    }
    r.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn reports_totals_admin_and_optout() {
    let mut pg = postgresql_embedded::PostgreSQL::default();
    pg.setup().await.unwrap();
    pg.start().await.unwrap();
    pg.create_database("stats").await.unwrap();
    let pool = sqlx::PgPool::connect(&pg.settings().url("stats")).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let app = router(pool.clone(), Config { admin_user: "admin".into(), admin_password: "s3cret".into(), geo: Geo::none(), rate_max: 5, site_dir: None });

    let a = "0123456789abcdef0123456789abcdef";
    let b = "fedcba9876543210fedcba9876543210";
    assert_eq!(call(&app, post(&report(a, 1000), "203.0.113.1")).await.0, StatusCode::NO_CONTENT);
    // every report is kept as its own row; nothing is merged or replaced
    assert_eq!(call(&app, post(&report(a, 3000), "203.0.113.1")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, post(&report(b, 7000), "203.0.113.2")).await.0, StatusCode::NO_CONTENT);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM reports").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 3);

    // v2: one event per thing cleaned; sending the same events again stores them once
    let v2 = json!({
        "v": 2, "install_id": b, "app_version": "0.8.0", "windows_build": "26300", "since": "2026-11-17T13:00Z",
        "events": [{"seq": 1, "at": "2026-11-17T13:42Z", "run": 1, "source": "gui", "area": "disk", "kind": "dir:node_modules", "bytes": 500, "count": 1},
                   {"seq": 2, "at": "2026-11-17T13:42Z", "run": 1, "source": "gui", "area": "memory", "kind": "mcp", "bytes": 100, "count": 4, "cpu": 7.5}],
        "machine": {"ram_total": 34359738368u64, "ram_used_avg": 17179869184u64, "cores": 16},
        "top_cpu": [], "top_mem": [], "tweaks": ["telemetry.diagtrack"],
        "seen": [{"kind": "process", "name": "ArmouryCrate.Service.exe", "display": "", "exe": "ArmouryCrate.Service.exe", "folder": "ASUS",
                  "keyword": "armoury", "running": true, "startup": false, "disabled": false, "cpu": 2.5, "mem": 90000000}]
    });
    assert_eq!(call(&app, post(&v2, "203.0.113.2")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, post(&v2, "203.0.113.2")).await.0, StatusCode::NO_CONTENT);
    let ev: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE seq IS NOT NULL").fetch_one(&pool).await.unwrap();
    assert_eq!(ev, 2);
    // vendor software rows are kept raw, one per report (the v2 report above was sent twice)
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM seen WHERE name = 'ArmouryCrate.Service.exe' AND folder = 'ASUS'").fetch_one(&pool).await.unwrap();
    assert_eq!(seen, 2);
    let mut leak = v2.clone();
    leak["seen"][0]["name"] = json!("client-secret-tool.exe");
    leak["seen"][0]["exe"] = json!("client-secret-tool.exe");
    leak["seen"][0]["folder"] = json!("");
    assert_eq!(call(&app, post(&leak, "203.0.113.4")).await.0, StatusCode::BAD_REQUEST, "a name without its keyword is refused");
    // b's earlier v1 totals are superseded by its row-by-row history, so nothing is counted twice
    let old: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE install_id = $1 AND seq IS NULL").bind(b).fetch_one(&pool).await.unwrap();
    assert_eq!(old, 0);
    let at: String = sqlx::query_scalar("SELECT to_char(at AT TIME ZONE 'UTC', 'YYYY-MM-DD HH24:MI') FROM events WHERE seq = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(at, "2026-11-17 13:42");
    let cpu: f32 = sqlx::query_scalar("SELECT cpu FROM events WHERE seq = 2").fetch_one(&pool).await.unwrap();
    assert_eq!(cpu, 7.5);

    // free text where a category belongs is refused
    let mut bad = report(a, 1);
    bad["freed"][0]["kind"] = json!(r"C:\Users\me\project");
    assert_eq!(call(&app, post(&bad, "203.0.113.3")).await.0, StatusCode::BAD_REQUEST);
    let mut extra = report(a, 1);
    extra["hostname"] = json!("my-pc");
    assert_eq!(call(&app, post(&extra, "203.0.113.3")).await.0, StatusCode::BAD_REQUEST);

    let (st, t) = call(&app, get("/v1/totals", None)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(t["installs"], 2);
    assert_eq!(t["disk_freed"], 1000 + 3000 + 500); // b: only its v2 rows
    assert_eq!(t["ram_recovered"], 2 * 500 + 100);
    assert_eq!(t["processes_ended"], 2 * 2 + 4);
    assert_eq!(t["cleanups"], 2 * 2 + 1); // a's v1 counts + b's one v2 run
    assert_eq!(t["top"][0]["kind"], "dir:target");

    // the dashboard needs the password
    assert_eq!(call(&app, get("/admin/api/summary", None)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, get("/admin/api/summary", Some("admin:wrong"))).await.0, StatusCode::UNAUTHORIZED);
    let (st, s) = call(&app, get("/admin/api/summary?days=30", Some("admin:s3cret"))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["days"], 30);
    assert_eq!(s["totals"]["now"]["active"], 2);
    assert_eq!(s["totals"]["now"]["reports"], 5);
    assert_eq!(s["totals"]["prev"]["active"], 0);
    assert_eq!(s["countries"][0]["country"], "??");
    assert_eq!(s["apps_cpu"][0]["app"], "VS Code");
    assert_eq!(s["tweaks"][0]["installs"], 2);
    assert_eq!(s["ram"][0]["name"], "32 GB");
    let series = s["series"].as_array().unwrap();
    assert!(series.len() >= 30);
    assert_eq!(series.last().unwrap()["active"], 2, "rows, not just dates: {}", series.last().unwrap());
    for days in ["1", "7", "90", "365", "all"] {
        let (st, s) = call(&app, get(&format!("/admin/api/summary?days={days}"), Some("admin:s3cret"))).await;
        assert_eq!(st, StatusCode::OK, "days={days}");
        assert_eq!(s["totals"]["now"]["active"], 2, "days={days}: {s}");
    }
    let (st, _) = call(&app, get("/admin", Some("admin:s3cret"))).await;
    assert_eq!(st, StatusCode::OK);

    // raw tables as CSV, owner only
    assert_eq!(call(&app, get("/admin/api/export/events.csv", None)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, get("/admin/api/export/pg_shadow.csv", Some("admin:s3cret"))).await.0, StatusCode::NOT_FOUND);
    let res = app.clone().oneshot(get("/admin/api/export/events.csv", Some("admin:s3cret"))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    let csv = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(csv.starts_with("id,report_id,install_id,seq,at,"));
    assert!(csv.contains("dir:node_modules"));
    let (_, s) = call(&app, get("/admin/api/summary?days=30", Some("admin:s3cret"))).await;
    let d = &s["discovered"][0];
    assert_eq!((d["name"].as_str(), d["installs"].as_i64(), d["folder"].as_str()), (Some("ArmouryCrate.Service.exe"), Some(1), Some("ASUS")));
    let res = app.clone().oneshot(get("/admin/api/export/seen.csv", Some("admin:s3cret"))).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // turning sharing off deletes everything that install sent
    let del = Request::delete(format!("/v1/install/{a}")).body(Body::empty()).unwrap();
    assert_eq!(call(&app, del).await.0, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE install_id = $1").bind(a).fetch_one(&pool).await.unwrap();
    assert_eq!(left, 0);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE install_id = $1").bind(a).fetch_one(&pool).await.unwrap();
    assert_eq!(left, 0);
    let (_, s) = call(&app, get("/admin/api/summary?days=30", Some("admin:s3cret"))).await;
    assert_eq!(s["totals"]["now"]["optouts"], 1);

    // rate limit: 5 per IP per 10 minutes here
    for _ in 0..5 {
        call(&app, post(&report(b, 1), "198.51.100.9")).await;
    }
    assert_eq!(call(&app, post(&report(b, 1), "198.51.100.9")).await.0, StatusCode::TOO_MANY_REQUESTS);

    // the browser extension: CORS preflight, two reports kept as two rows, and opt-out
    let pre = Request::options("/v1/ext/report")
        .header(header::ORIGIN, "chrome-extension://abcdefghijklmnop")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(pre).await.unwrap();
    assert!(res.status().is_success());
    assert_eq!(res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    let e = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let ext = |slept: u64| {
        let v = json!({"v": 1, "install_id": e, "ext_version": "1.1.0", "browser": "Chrome", "browser_major": 131,
            "tabs_slept": slept, "dupes_closed": 2, "matches_closed": 1, "tabs_open": 57, "windows_open": 3, "auto_sleep": true});
        Request::post("/v1/ext/report")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-real-ip", "192.0.2.7")
            .body(Body::from(v.to_string()))
            .unwrap()
    };
    assert_eq!(call(&app, ext(40)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, ext(10)).await.0, StatusCode::NO_CONTENT);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM ext_reports").fetch_one(&pool).await.unwrap();
    assert_eq!(rows, 2);
    let (_, s) = call(&app, get("/admin/api/summary?days=7", Some("admin:s3cret"))).await;
    assert_eq!(s["totals"]["now"]["tabs_slept"], 50);
    assert_eq!(s["totals"]["now"]["tabs_closed"], 6);
    assert_eq!(s["ext_browsers"][0]["name"], "Chrome");
    assert_eq!(s["ext_tabs"][0]["name"], "31–60");
    let del = Request::delete(format!("/v1/install/{e}")).body(Body::empty()).unwrap();
    assert_eq!(call(&app, del).await.0, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM ext_reports").fetch_one(&pool).await.unwrap();
    assert_eq!(left, 0);

    // no IP address is stored anywhere
    let dump: String = sqlx::query_scalar(
        "SELECT string_agg(t::text, ' ') FROM (SELECT row_to_json(i)::text AS t FROM installs i UNION ALL SELECT row_to_json(r)::text FROM reports r
          UNION ALL SELECT row_to_json(e)::text FROM ext_installs e) x",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!dump.contains("203.0.113") && !dump.contains("198.51.100"));

    pool.close().await;
    pg.stop().await.unwrap();
}
