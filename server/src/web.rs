//! HTTP: the report endpoint, opt-out deletion, public counters and the owner's dashboard.
//! The client IP is used for two things only, in memory: a rate limit and a country lookup.
//! It is never stored or logged.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, Extensions, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use base64::Engine;
use serde_json::Value;
use sqlx::PgPool;
use subtle::ConstantTimeEq;

use crate::model::{install_id_ok, ExtReport, Report};
use crate::store;

pub struct Geo {
    reader: Option<maxminddb::Reader<Vec<u8>>>,
    /// behind Cloudflare: take its CF-IPCountry header
    trust_cf: bool,
}

impl Geo {
    pub fn new(mmdb: Option<&str>, trust_cf: bool) -> anyhow::Result<Geo> {
        let reader = match mmdb {
            Some(p) if !p.is_empty() => Some(maxminddb::Reader::open_readfile(p)?),
            _ => None,
        };
        Ok(Geo { reader, trust_cf })
    }

    pub fn none() -> Geo {
        Geo { reader: None, trust_cf: false }
    }

    /// ISO country code, or "??".
    fn country(&self, headers: &HeaderMap, ip: Option<IpAddr>) -> String {
        let ok = |c: &str| c.len() == 2 && c.chars().all(|ch| ch.is_ascii_uppercase());
        if self.trust_cf {
            if let Some(c) = headers.get("cf-ipcountry").and_then(|v| v.to_str().ok()) {
                if ok(c) && c != "XX" && c != "T1" {
                    return c.to_string();
                }
            }
        }
        if let (Some(r), Some(ip)) = (&self.reader, ip) {
            if let Ok(c) = r.lookup::<maxminddb::geoip2::Country>(ip) {
                if let Some(code) = c.country.and_then(|c| c.iso_code) {
                    if ok(code) {
                        return code.to_string();
                    }
                }
            }
        }
        "??".into()
    }
}

/// N requests per window per IP, in memory only.
struct Limiter {
    hits: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    max: u32,
    window: Duration,
}

impl Limiter {
    fn allow(&self, ip: IpAddr) -> bool {
        let Ok(mut m) = self.hits.lock() else { return true };
        let now = Instant::now();
        if m.len() > 50_000 {
            m.retain(|_, (_, t)| now.duration_since(*t) < self.window);
        }
        let e = m.entry(ip).or_insert((0, now));
        if now.duration_since(e.1) >= self.window {
            *e = (0, now);
        }
        e.0 += 1;
        e.0 <= self.max
    }
}

pub struct Config {
    pub admin_user: String,
    /// empty = the dashboard is off
    pub admin_password: String,
    pub geo: Geo,
    /// reports allowed per IP per 10 minutes
    pub rate_max: u32,
    /// the static site (rigsweep.com, the repo's docs/ folder); None = API only
    pub site_dir: Option<std::path::PathBuf>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    pool: PgPool,
    cfg: Config,
    limiter: Limiter,
    totals: Mutex<Option<(Instant, Value)>>,
}

pub fn router(pool: PgPool, cfg: Config) -> Router {
    let site = cfg.site_dir.clone();
    let limiter = Limiter { hits: Mutex::new(HashMap::new()), max: cfg.rate_max, window: Duration::from_secs(600) };
    let st = AppState(Arc::new(Inner { pool, cfg, limiter, totals: Mutex::new(None) }));
    // the browser extension calls these from its own origin: open CORS, no cookies involved
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([axum::http::Method::POST, axum::http::Method::DELETE])
        .allow_headers([header::CONTENT_TYPE])
        .max_age(Duration::from_secs(86400));
    let open = Router::new()
        .route("/v1/ext/report", post(ext_report))
        .route("/v1/install/{id}", delete(forget))
        .layer(cors);
    let mut app = Router::new()
        .route("/v1/report", post(report))
        .merge(open)
        .route("/v1/totals", get(totals))
        .route("/admin", get(admin_page))
        .route("/admin/api/summary", get(admin_summary))
        .route("/admin/api/export/{table}", get(admin_export))
        .route("/healthz", get(|| async { "ok" }))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .with_state(st);
    if let Some(dir) = site {
        // everything that isn't an API route is a file from the site; unknown paths get 404.html with status 404
        let files = tower_http::services::ServeDir::new(&dir).not_found_service(tower_http::services::ServeFile::new(dir.join("404.html")));
        let files = tower::ServiceBuilder::new()
            .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
            .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                header::REFERRER_POLICY,
                HeaderValue::from_static("strict-origin-when-cross-origin"),
            ))
            .service(files);
        app = app.fallback_service(files);
    }
    app.layer(axum::middleware::from_fn(www_redirect))
}

/// www.example.com/anything -> https://example.com/anything (301).
/// stats.example.com serves only the API, the dashboard and the health check; anything else
/// (like its bare address) goes to the site: stats.example.com/x -> https://example.com/x (301).
async fn www_redirect(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    let path = req.uri().path();
    let api = path.starts_with("/v1/") || path == "/admin" || path.starts_with("/admin/") || path == "/healthz";
    let target = host.strip_prefix("www.").or_else(|| host.strip_prefix("stats.").filter(|_| !api));
    if let Some(bare) = target {
        let pq = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("/");
        if let Ok(loc) = HeaderValue::from_str(&format!("https://{bare}{pq}")) {
            return (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, loc)]).into_response();
        }
    }
    next.run(req).await
}

/// The server listens on localhost only and is reached through the Cloudflare tunnel, so
/// Cloudflare's CF-Connecting-IP is the visitor. X-Real-IP is kept for other proxies and tests.
fn client_ip(headers: &HeaderMap, ext: &Extensions) -> Option<IpAddr> {
    ["cf-connecting-ip", "x-real-ip"]
        .iter()
        .find_map(|h| headers.get(*h).and_then(|v| v.to_str().ok()).and_then(|s| s.trim().parse().ok()))
        .or_else(|| ext.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip()))
}

fn fail(e: sqlx::Error) -> Response {
    tracing::error!("database: {e}");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

async fn report(State(st): State<AppState>, headers: HeaderMap, ext: Extensions, body: axum::body::Bytes) -> Response {
    let ip = client_ip(&headers, &ext);
    if let Some(ip) = ip {
        if !st.0.limiter.allow(ip) {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }
    let r: Report = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    if let Err(e) = r.validate() {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    let country = st.0.cfg.geo.country(&headers, ip);
    match store::save_report(&st.0.pool, &r, &country).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fail(e),
    }
}

async fn ext_report(State(st): State<AppState>, headers: HeaderMap, ext: Extensions, body: axum::body::Bytes) -> Response {
    let ip = client_ip(&headers, &ext);
    if let Some(ip) = ip {
        if !st.0.limiter.allow(ip) {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
    }
    let r: ExtReport = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid json").into_response(),
    };
    if let Err(e) = r.validate() {
        return (StatusCode::BAD_REQUEST, e).into_response();
    }
    let country = st.0.cfg.geo.country(&headers, ip);
    match store::save_ext_report(&st.0.pool, &r, &country).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fail(e),
    }
}

async fn forget(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    if !install_id_ok(&id) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match store::delete_install(&st.0.pool, &id).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(), // the same answer whether it existed or not
        Err(e) => fail(e),
    }
}

async fn totals(State(st): State<AppState>) -> Response {
    let cached = st.0.totals.lock().ok().and_then(|c| c.clone()).filter(|(t, _)| t.elapsed() < Duration::from_secs(300));
    let v = match cached {
        Some((_, v)) => v,
        None => match store::public_totals(&st.0.pool).await {
            Ok(v) => {
                if let Ok(mut c) = st.0.totals.lock() {
                    *c = Some((Instant::now(), v.clone()));
                }
                v
            }
            Err(e) => return fail(e),
        },
    };
    let mut res = Json(v).into_response();
    let h = res.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=300"));
    res
}

/// HTTP Basic auth against ADMIN_USER / ADMIN_PASSWORD (HTTPS only, via Cloudflare). Off when no password is set.
fn admin_ok(st: &AppState, headers: &HeaderMap) -> Result<(), Response> {
    let cfg = &st.0.cfg;
    if cfg.admin_password.is_empty() {
        return Err(StatusCode::NOT_FOUND.into_response());
    }
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b| base64::engine::general_purpose::STANDARD.decode(b.trim()).ok())
        .unwrap_or_default();
    let want = format!("{}:{}", cfg.admin_user, cfg.admin_password);
    if given.len() == want.len() && bool::from(given.as_slice().ct_eq(want.as_bytes())) {
        return Ok(());
    }
    Err((StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, "Basic realm=\"RigSweep admin\", charset=\"UTF-8\"")]).into_response())
}

fn private<R: IntoResponse>(r: R) -> Response {
    let mut res = r.into_response();
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert("x-robots-tag", HeaderValue::from_static("noindex"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'"),
    );
    res
}

async fn admin_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(r) = admin_ok(&st, &headers) {
        return r;
    }
    private(Html(include_str!("admin.html")))
}

#[derive(serde::Deserialize)]
struct Range {
    /// 1, 7, 30, 90 or 365; anything else ("all", nothing) = all time
    days: Option<String>,
}

async fn admin_summary(State(st): State<AppState>, headers: HeaderMap, axum::extract::Query(q): axum::extract::Query<Range>) -> Response {
    if let Err(r) = admin_ok(&st, &headers) {
        return r;
    }
    let days = q.days.and_then(|d| d.parse::<u32>().ok()).filter(|d| [1, 7, 30, 90, 365].contains(d));
    match store::admin_summary(&st.0.pool, days).await {
        Ok(v) => private(Json(v)),
        Err(e) => fail(e),
    }
}

/// A raw table as CSV, e.g. /admin/api/export/events.csv
async fn admin_export(State(st): State<AppState>, headers: HeaderMap, Path(table): Path<String>) -> Response {
    if let Err(r) = admin_ok(&st, &headers) {
        return r;
    }
    let table = table.strip_suffix(".csv").unwrap_or(&table);
    if !store::EXPORTS.contains(&table) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match store::export_csv(&st.0.pool, table).await {
        Ok(csv) => {
            let mut res = private(csv);
            let h = res.headers_mut();
            h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8"));
            if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"rigsweep-{table}.csv\"")) {
                h.insert(header::CONTENT_DISPOSITION, v);
            }
            res
        }
        Err(e) => fail(e),
    }
}
