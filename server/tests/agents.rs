//! What agents and API clients see: Markdown content negotiation, RFC 9457 JSON errors, RateLimit
//! headers, the API catalog, the docs redirects, and docs/openapi.json matching the real routes.
//! No database needed: the pool points nowhere and gives up fast.

use std::time::Duration;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use rigsweep_stats::web::prefers_markdown;
use rigsweep_stats::{router, Config, Geo};

fn site() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rigsweep-agents-{}-{:?}", std::process::id(), std::thread::current().id()));
    for d in ["about", "cli", ".well-known"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(dir.join("index.html"), "<h1>home</h1>").unwrap();
    std::fs::write(dir.join("index.md"), "# home\n").unwrap();
    std::fs::write(dir.join("about/index.html"), "<h1>about</h1>").unwrap();
    std::fs::write(dir.join("about/index.md"), "# about\n").unwrap();
    std::fs::write(dir.join("cli/index.html"), "<h1>cli</h1>").unwrap(); // no Markdown twin
    std::fs::write(dir.join(".well-known/api-catalog"), r#"{"linkset":[]}"#).unwrap();
    std::fs::write(dir.join("openapi.json"), "{}").unwrap();
    std::fs::write(dir.join("404.html"), "missing").unwrap();
    dir
}

fn app(site_dir: Option<std::path::PathBuf>, rate_max: u32) -> axum::Router {
    let pool = sqlx::postgres::PgPoolOptions::new().acquire_timeout(Duration::from_millis(300)).connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap();
    router(pool, Config { admin_user: "admin".into(), admin_password: String::new(), geo: Geo::none(), rate_max, site_dir })
}

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Res {
    fn h(&self, k: &str) -> &str {
        self.headers.get(k).map(|v| v.to_str().unwrap()).unwrap_or("")
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
    /// an RFC 9457 problem with all the members the docs promise
    fn assert_problem(&self, status: StatusCode, code: &str) {
        assert_eq!(self.status, status, "{}", self.body);
        assert_eq!(self.h("content-type"), "application/problem+json");
        let p = self.json();
        assert_eq!(p["status"], status.as_u16());
        assert_eq!(p["code"], code);
        assert_eq!(p["type"], format!("https://rigsweep.com/developers/#error-{code}"));
        for k in ["title", "detail", "hint"] {
            assert!(p[k].as_str().is_some_and(|s| !s.is_empty()), "{k} missing: {p}");
        }
    }
}

async fn send(app: &axum::Router, req: Request<Body>) -> Res {
    let res = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let body = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).into_owned();
    Res { status, headers, body }
}

fn get(path: &str, accept: Option<&str>) -> Request<Body> {
    let mut r = Request::get(path).header(header::HOST, "rigsweep.com");
    if let Some(a) = accept {
        r = r.header(header::ACCEPT, a);
    }
    r.body(Body::empty()).unwrap()
}

fn post(path: &str, body: impl Into<Body>, ip: &str) -> Request<Body> {
    Request::post(path).header(header::CONTENT_TYPE, "application/json").header("x-real-ip", ip).body(body.into()).unwrap()
}

fn accept(a: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::ACCEPT, a.parse().unwrap());
    h
}

#[test]
fn accept_header_ranking() {
    assert!(prefers_markdown(&accept("text/markdown")));
    assert!(prefers_markdown(&accept("text/markdown, text/html;q=0.9")));
    assert!(prefers_markdown(&accept("text/markdown, */*;q=0.1")));
    assert!(prefers_markdown(&accept("text/html;q=0.5, text/markdown")));
    assert!(prefers_markdown(&accept("TEXT/Markdown; charset=utf-8")));
    // browsers and generic clients get HTML
    assert!(!prefers_markdown(&accept("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")));
    assert!(!prefers_markdown(&accept("*/*")));
    assert!(!prefers_markdown(&HeaderMap::new()));
    assert!(!prefers_markdown(&accept("text/markdown;q=0.5, text/html")));
    assert!(!prefers_markdown(&accept("text/markdown;q=0")));
}

#[tokio::test]
async fn markdown_negotiation() {
    let app = app(Some(site()), 30);

    for path in ["/", "/about/"] {
        let md = send(&app, get(path, Some("text/markdown"))).await;
        assert_eq!(md.status, StatusCode::OK, "{path}");
        assert_eq!(md.h("content-type"), "text/markdown; charset=utf-8", "{path}");
        assert_eq!(md.h("vary"), "Accept", "{path}");
        assert!(md.body.starts_with("# "), "{path}: {}", md.body);

        for a in [Some("text/html"), Some("text/html,application/xhtml+xml,*/*;q=0.8"), None] {
            let html = send(&app, get(path, a)).await;
            assert_eq!(html.status, StatusCode::OK);
            assert!(html.h("content-type").starts_with("text/html"), "{path} {a:?}");
            assert!(html.headers.get_all(header::VARY).iter().any(|v| v == "Accept"), "{path} {a:?}: Vary");
            assert!(html.body.starts_with("<h1>"));
        }
    }

    // HEAD answers like GET
    let head = Request::head("/").header(header::HOST, "rigsweep.com").header(header::ACCEPT, "text/markdown").body(Body::empty()).unwrap();
    let r = send(&app, head).await;
    assert_eq!((r.status, r.h("content-type")), (StatusCode::OK, "text/markdown; charset=utf-8"));

    // a page without a Markdown twin stays HTML; the .md file itself is Markdown with a charset
    let r = send(&app, get("/cli/", Some("text/markdown"))).await;
    assert!(r.h("content-type").starts_with("text/html"));
    let r = send(&app, get("/index.md", None)).await;
    assert_eq!(r.h("content-type"), "text/markdown; charset=utf-8");
    // hidden folders and unknown pages aren't negotiated
    assert_eq!(send(&app, get("/.well-known/", Some("text/markdown"))).await.status, StatusCode::NOT_FOUND);
    assert_eq!(send(&app, get("/nope/", Some("text/markdown"))).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn discovery_files_and_redirects() {
    let app = app(Some(site()), 30);
    let r = send(&app, get("/.well-known/api-catalog", None)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.h("content-type"), "application/linkset+json; profile=\"https://www.rfc-editor.org/info/rfc9727\"");
    assert_eq!(r.h("link"), "</.well-known/api-catalog>; rel=\"api-catalog\"");
    let r = send(&app, get("/openapi.json", None)).await;
    assert_eq!((r.status, r.h("content-type")), (StatusCode::OK, "application/json"));
    for p in ["/docs", "/docs/", "/api", "/api/"] {
        let r = send(&app, get(p, None)).await;
        assert_eq!((r.status, r.h("location")), (StatusCode::PERMANENT_REDIRECT, "/developers/"), "{p}");
    }
}

#[tokio::test]
async fn api_errors_are_problem_json() {
    let app = app(Some(site()), 30);
    // unknown API paths are JSON even with the site mounted (not 404.html)
    send(&app, get("/v1/nope", None)).await.assert_problem(StatusCode::NOT_FOUND, "not_found");
    let r = send(&app, get("/v1/report", None)).await;
    r.assert_problem(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
    assert!(r.h("allow").contains("POST"), "Allow kept: {:?}", r.headers);
    send(&app, post("/v1/report", "not json", "192.0.2.1")).await.assert_problem(StatusCode::BAD_REQUEST, "invalid_json");
    send(&app, post("/v1/ext/report", "{}", "192.0.2.1")).await.assert_problem(StatusCode::BAD_REQUEST, "invalid_json");
    let bad = r#"{"v":1,"install_id":"nope","ext_version":"1.0","browser":"Chrome","browser_major":1,"tabs_slept":0,"dupes_closed":0,"matches_closed":0,"tabs_open":1,"windows_open":1,"auto_sleep":false}"#;
    let r = send(&app, post("/v1/ext/report", bad, "192.0.2.1")).await;
    r.assert_problem(StatusCode::BAD_REQUEST, "invalid_field");
    assert_eq!(r.json()["detail"], "invalid install_id");
    send(&app, post("/v1/report", vec![b' '; 40 * 1024], "192.0.2.1")).await.assert_problem(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large");
    let del = Request::delete("/v1/install/NOT-HEX").body(Body::empty()).unwrap();
    send(&app, del).await.assert_problem(StatusCode::BAD_REQUEST, "invalid_install_id");
    // the database is unreachable here: a 500 is a problem too, and nothing leaks
    let r = send(&app, get("/v1/totals", None)).await;
    r.assert_problem(StatusCode::INTERNAL_SERVER_ERROR, "server_error");
    assert!(!r.body.contains("127.0.0.1"));
    // the owner's endpoints stay hidden, in JSON
    send(&app, get("/admin/api/summary", None)).await.assert_problem(StatusCode::NOT_FOUND, "not_found");
    // site pages keep their HTML 404
    let r = send(&app, get("/nope", None)).await;
    assert_eq!((r.status, r.body.as_str()), (StatusCode::NOT_FOUND, "missing"));
}

#[tokio::test]
async fn rate_limit_headers() {
    let app = app(None, 3);
    for left in (0..3).rev() {
        let r = send(&app, post("/v1/report", "{}", "198.51.100.4")).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST);
        assert_eq!(r.h("ratelimit-policy"), "\"reports\";q=3;w=600");
        assert!(r.h("ratelimit").starts_with(&format!("\"reports\";r={left};t=")), "{}", r.h("ratelimit"));
        assert!(r.h("retry-after").is_empty());
    }
    let r = send(&app, post("/v1/report", "{}", "198.51.100.4")).await;
    r.assert_problem(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    let wait: u64 = r.h("retry-after").parse().unwrap();
    assert!((1..=600).contains(&wait));
    assert_eq!(r.h("ratelimit"), format!("\"reports\";r=0;t={wait}"));
    // the limit is per address
    assert_eq!(send(&app, post("/v1/report", "{}", "198.51.100.5")).await.status, StatusCode::BAD_REQUEST);
}

/// Every operation in docs/openapi.json is a real route that answers with what the spec says:
/// its documented status codes, and problem+json for every error.
#[tokio::test]
async fn openapi_matches_routes() {
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/openapi.json")).unwrap()).unwrap();
    let app = app(None, 1000);
    let mut ops = 0;
    for (path, item) in spec["paths"].as_object().unwrap() {
        for (method, op) in item.as_object().unwrap() {
            ops += 1;
            let uri = path.replace("{installId}", "0123456789abcdef0123456789abcdef");
            let m: Method = method.to_uppercase().parse().unwrap();
            let body = if m == Method::POST { "{}" } else { "" };
            let req = Request::builder().method(m).uri(&uri).header("x-real-ip", "203.0.113.50").header(header::CONTENT_TYPE, "application/json").body(Body::from(body)).unwrap();
            let r = send(&app, req).await;
            let documented = op["responses"].as_object().unwrap();
            assert!(documented.contains_key(r.status.as_str()), "{method} {path}: {} isn't in the spec", r.status);
            if r.status.is_client_error() || r.status.is_server_error() {
                assert_eq!(r.h("content-type"), "application/problem+json", "{method} {path}");
                let code = r.json()["code"].as_str().unwrap().to_string();
                let codes = &spec["components"]["schemas"]["Problem"]["properties"]["code"]["enum"];
                assert!(codes.as_array().unwrap().iter().any(|c| c == code.as_str()), "{code} isn't in the spec's enum");
            }
        }
    }
    assert_eq!(ops, 5);
}
