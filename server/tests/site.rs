//! The static site, the www redirect and the health check. No database needed: the pool is lazy
//! and these routes never touch it.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use rigsweep_stats::{router, Config, Geo};

fn app() -> axum::Router {
    let dir = std::env::temp_dir().join(format!("rigsweep-site-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("about")).unwrap();
    std::fs::write(dir.join("index.html"), "home").unwrap();
    std::fs::write(dir.join("about/index.html"), "about").unwrap();
    std::fs::write(dir.join("404.html"), "missing").unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap();
    router(pool, Config { admin_user: "admin".into(), admin_password: String::new(), geo: Geo::none(), rate_max: 5, site_dir: Some(dir) })
}

async fn get(app: &axum::Router, host: &str, path: &str) -> (StatusCode, String, Option<String>) {
    let req = Request::get(path).header(header::HOST, host).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let loc = res.headers().get(header::LOCATION).map(|v| v.to_str().unwrap().to_string());
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    (status, body, loc)
}

#[tokio::test]
async fn site_redirect_and_health() {
    let app = app();
    assert_eq!(get(&app, "rigsweep.com", "/").await, (StatusCode::OK, "home".into(), None));
    assert_eq!(get(&app, "rigsweep.com", "/about/").await.1, "about");
    let (s, body, _) = get(&app, "rigsweep.com", "/nope").await;
    assert_eq!((s, body.as_str()), (StatusCode::NOT_FOUND, "missing"));
    assert_eq!(get(&app, "stats.rigsweep.com", "/healthz").await.1, "ok");
    let (s, _, loc) = get(&app, "www.rigsweep.com", "/about/?x=1").await;
    assert_eq!((s, loc.as_deref()), (StatusCode::MOVED_PERMANENTLY, Some("https://rigsweep.com/about/?x=1")));
    // stats.* is API-only: its bare address and site pages go to the main domain
    let (s, _, loc) = get(&app, "stats.rigsweep.com", "/").await;
    assert_eq!((s, loc.as_deref()), (StatusCode::MOVED_PERMANENTLY, Some("https://rigsweep.com/")));
    assert_eq!(get(&app, "stats.rigsweep.com", "/about/").await.2.as_deref(), Some("https://rigsweep.com/about/"));
    assert_eq!(get(&app, "stats.rigsweep.com", "/v1/nope").await.0, StatusCode::NOT_FOUND);
    // the dashboard stays off without a password, even with the site mounted
    assert_eq!(get(&app, "stats.rigsweep.com", "/admin").await.0, StatusCode::NOT_FOUND);
}
