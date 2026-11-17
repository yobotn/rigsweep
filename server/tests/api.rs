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
    // a second report in the same week replaces the first, it doesn't add to it
    assert_eq!(call(&app, post(&report(a, 3000), "203.0.113.1")).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, post(&report(b, 7000), "203.0.113.2")).await.0, StatusCode::NO_CONTENT);

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
    assert_eq!(t["disk_freed"], 10000);
    assert_eq!(t["ram_recovered"], 1000);
    assert_eq!(t["top"][0]["kind"], "dir:target");

    // the dashboard needs the password
    assert_eq!(call(&app, get("/admin/api/summary", None)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, get("/admin/api/summary", Some("admin:wrong"))).await.0, StatusCode::UNAUTHORIZED);
    let (st, s) = call(&app, get("/admin/api/summary", Some("admin:s3cret"))).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(s["totals"]["active_30d"], 2);
    assert_eq!(s["countries"][0]["country"], "??");
    assert_eq!(s["apps_cpu"][0]["app"], "VS Code");
    assert_eq!(s["tweaks"][0]["installs"], 2);
    assert_eq!(s["ram"][0]["name"], "32 GB");
    let (st, _) = call(&app, get("/admin", Some("admin:s3cret"))).await;
    assert_eq!(st, StatusCode::OK);

    // turning sharing off deletes everything that install sent
    let del = Request::delete(format!("/v1/install/{a}")).body(Body::empty()).unwrap();
    assert_eq!(call(&app, del).await.0, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE install_id = $1").bind(a).fetch_one(&pool).await.unwrap();
    assert_eq!(left, 0);
    let (_, s) = call(&app, get("/admin/api/summary", Some("admin:s3cret"))).await;
    assert_eq!(s["totals"]["optouts_30d"], 1);

    // rate limit: 5 per IP per 10 minutes here
    for _ in 0..5 {
        call(&app, post(&report(b, 1), "198.51.100.9")).await;
    }
    assert_eq!(call(&app, post(&report(b, 1), "198.51.100.9")).await.0, StatusCode::TOO_MANY_REQUESTS);

    // the browser extension: CORS preflight, a report, counts adding up within a week, and opt-out
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
    let (_, s) = call(&app, get("/admin/api/summary", Some("admin:s3cret"))).await;
    assert_eq!(s["totals"]["tabs_slept"], 50);
    assert_eq!(s["totals"]["tabs_closed"], 6);
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
