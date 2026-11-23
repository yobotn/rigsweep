//! HTTP: the report endpoint, opt-out deletion, public counters and the owner's dashboard, plus the
//! static site. API errors are RFC 9457 problem details (JSON); the report endpoints send the IETF
//! RateLimit / RateLimit-Policy headers. The public API is described in docs/openapi.json.
//! The client IP is used for two things only, in memory: a rate limit and a country lookup.
//! It is never stored or logged.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, Extensions, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{any, delete, get, post};
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
            if let Some(c) = r.lookup(ip).ok().and_then(|res| res.decode::<maxminddb::geoip2::Country>().ok().flatten()) {
                if let Some(code) = c.country.iso_code {
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

/// The outcome of one request against the limit, sent back as RateLimit headers.
struct Quota {
    ok: bool,
    remaining: u32,
    /// seconds until the window starts over
    reset: u64,
}

impl Limiter {
    fn check(&self, ip: IpAddr) -> Quota {
        let window = self.window.as_secs();
        let Ok(mut m) = self.hits.lock() else { return Quota { ok: true, remaining: self.max, reset: window } };
        let now = Instant::now();
        if m.len() > 50_000 {
            m.retain(|_, (_, t)| now.duration_since(*t) < self.window);
        }
        let e = m.entry(ip).or_insert((0, now));
        if now.duration_since(e.1) >= self.window {
            *e = (0, now);
        }
        e.0 = e.0.saturating_add(1);
        let reset = window.saturating_sub(now.duration_since(e.1).as_secs()).max(1);
        Quota { ok: e.0 <= self.max, remaining: self.max.saturating_sub(e.0), reset }
    }

    /// RateLimit-Policy / RateLimit (draft-ietf-httpapi-ratelimit-headers), plus Retry-After when over.
    fn headers(&self, q: &Quota) -> HeaderMap {
        let mut h = HeaderMap::new();
        let policy = format!("\"reports\";q={};w={}", self.max, self.window.as_secs());
        let now = format!("\"reports\";r={};t={}", q.remaining, q.reset);
        for (k, v) in [("ratelimit-policy", policy), ("ratelimit", now)] {
            if let Ok(v) = HeaderValue::from_str(&v) {
                h.insert(k, v);
            }
        }
        if !q.ok {
            h.insert(header::RETRY_AFTER, HeaderValue::from(q.reset));
        }
        h
    }
}

/// An RFC 9457 problem details response. `code` is stable for programs; `hint` says how to fix it.
/// The `type` URI points at the error's entry on the developer page.
pub fn problem(status: StatusCode, code: &str, detail: &str, hint: &str) -> Response {
    let body = serde_json::json!({
        "type": format!("https://rigsweep.com/developers/#error-{code}"),
        "title": status.canonical_reason().unwrap_or("Error"),
        "status": status.as_u16(),
        "detail": detail,
        "code": code,
        "hint": hint,
    });
    (status, [(header::CONTENT_TYPE, HeaderValue::from_static("application/problem+json"))], body.to_string()).into_response()
}

/// The generic problem for an error status that had no JSON body of its own.
fn problem_for(status: StatusCode) -> Response {
    let (code, detail, hint) = match status {
        StatusCode::NOT_FOUND => ("not_found", "There is no such API endpoint.", "See https://rigsweep.com/openapi.json for the endpoints."),
        StatusCode::METHOD_NOT_ALLOWED => ("method_not_allowed", "This endpoint doesn't take that HTTP method.", "The Allow header lists the methods it takes."),
        StatusCode::PAYLOAD_TOO_LARGE => ("payload_too_large", "The request body is larger than 32 KiB.", "Send fewer events per report."),
        StatusCode::UNAUTHORIZED => ("unauthorized", "This endpoint needs the owner's credentials.", "Only the public endpoints in https://rigsweep.com/openapi.json are open."),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ("unsupported_media_type", "The request body has the wrong content type.", "Send Content-Type: application/json."),
        s if s.is_server_error() => ("server_error", "The server couldn't complete the request.", "Retry later; nothing was stored."),
        _ => ("error", "The request failed.", "See https://rigsweep.com/developers/ for how to call the API."),
    };
    problem(status, code, detail, hint)
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
    let site = cfg.site_dir.clone().map(Arc::new);
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
        let files = tower_http::services::ServeDir::new(dir.as_path()).not_found_service(tower_http::services::ServeFile::new(dir.join("404.html")));
        let files = tower::ServiceBuilder::new()
            .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
            .layer(tower_http::set_header::SetResponseHeaderLayer::if_not_present(
                header::REFERRER_POLICY,
                HeaderValue::from_static("strict-origin-when-cross-origin"),
            ))
            .service(files);
        // the usual places agents look for docs all lead to the developer page
        let to_dev = || any(|| async { (StatusCode::PERMANENT_REDIRECT, [(header::LOCATION, "/developers/")]) });
        app = app
            .route("/docs", to_dev())
            .route("/docs/", to_dev())
            .route("/api", to_dev())
            .route("/api/", to_dev())
            .fallback_service(files)
            .layer(axum::middleware::from_fn_with_state(dir, negotiate));
    }
    app.layer(axum::middleware::from_fn(api_errors)).layer(axum::middleware::from_fn(www_redirect))
}

fn is_api(path: &str) -> bool {
    path.starts_with("/v1/") || path == "/v1" || path.starts_with("/admin/api/")
}

/// Every error from an API path is problem+json, including the ones axum makes itself (unknown
/// route, wrong method, body too large) and unknown /v1/ paths that would otherwise get 404.html.
async fn api_errors(req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    if !is_api(req.uri().path()) {
        return next.run(req).await;
    }
    let res = next.run(req).await;
    let status = res.status();
    let is_problem = res.headers().get(header::CONTENT_TYPE).is_some_and(|v| v.as_bytes().starts_with(b"application/problem+json"));
    if !(status.is_client_error() || status.is_server_error()) || is_problem {
        return res;
    }
    let (parts, _) = res.into_parts();
    let mut out = problem_for(status);
    // keep Allow, WWW-Authenticate, Retry-After, CORS and the rest; only the body and its type change
    for (k, v) in parts.headers.iter() {
        if k != header::CONTENT_TYPE && k != header::CONTENT_LENGTH && !out.headers().contains_key(k) {
            out.headers_mut().append(k.clone(), v.clone());
        }
    }
    out
}

/// True when the Accept header ranks text/markdown at least as high as text/html (by q-value;
/// text/html falls back to text/* and */*). Browsers never list text/markdown, so they get HTML.
pub fn prefers_markdown(headers: &HeaderMap) -> bool {
    let accept = headers.get_all(header::ACCEPT).iter().filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join(",");
    let (mut md, mut html, mut text_any, mut any) = (None::<f32>, None::<f32>, None::<f32>, None::<f32>);
    for range in accept.split(',') {
        let mut parts = range.split(';');
        let mime = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let q = parts
            .filter_map(|p| {
                let (k, v) = p.split_once('=')?;
                k.trim().eq_ignore_ascii_case("q").then(|| v.trim().parse::<f32>().ok()).flatten()
            })
            .next()
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let slot = match mime.as_str() {
            "text/markdown" => &mut md,
            "text/html" => &mut html,
            "text/*" => &mut text_any,
            "*/*" => &mut any,
            _ => continue,
        };
        *slot = Some(slot.map_or(q, |old: f32| old.max(q)));
    }
    let md = md.unwrap_or(0.0);
    md > 0.0 && md >= html.or(text_any).or(any).unwrap_or(0.0)
}

/// Markdown content negotiation for the site: a page URL (ending in /) whose folder has an
/// index.md is sent as Markdown to clients that ask for text/markdown, and as HTML otherwise;
/// both answers carry `Vary: Accept`. Also gives .md files a charset and the RFC 9727 API catalog
/// its media type.
async fn negotiate(State(dir): State<Arc<std::path::PathBuf>>, req: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let path = req.uri().path().to_string();
    let read = req.method() == axum::http::Method::GET || req.method() == axum::http::Method::HEAD;
    if !read || is_api(&path) {
        return next.run(req).await;
    }
    if path == "/.well-known/api-catalog" {
        let mut res = next.run(req).await;
        if res.status().is_success() {
            res.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/linkset+json; profile=\"https://www.rfc-editor.org/info/rfc9727\""),
            );
            res.headers_mut().insert(header::LINK, HeaderValue::from_static("</.well-known/api-catalog>; rel=\"api-catalog\""));
        }
        return res;
    }
    if path.ends_with(".md") {
        let mut res = next.run(req).await;
        if res.status().is_success() {
            res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/markdown; charset=utf-8"));
        }
        return res;
    }
    let rel = path.trim_start_matches('/');
    let safe = path.ends_with('/') && !rel.contains('\\') && rel.split('/').all(|s| s.is_empty() || !s.starts_with('.'));
    let md = dir.join(rel).join("index.md");
    if !safe || !md.is_file() {
        return next.run(req).await;
    }
    let vary = HeaderValue::from_static("Accept");
    if prefers_markdown(req.headers()) {
        if let Ok(body) = tokio::fs::read(&md).await {
            let mut res = (StatusCode::OK, [(header::CONTENT_TYPE, HeaderValue::from_static("text/markdown; charset=utf-8"))], body).into_response();
            let h = res.headers_mut();
            h.insert(header::VARY, vary);
            h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
            return res;
        }
    }
    let mut res = next.run(req).await;
    res.headers_mut().append(header::VARY, vary);
    res
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
    problem(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "The database couldn't be reached.", "Retry later; nothing was stored.")
}

/// Runs the rate limit for a report. Err = the 429 to send; Ok = headers to add to the answer.
fn limit(st: &AppState, ip: Option<IpAddr>) -> Result<HeaderMap, Response> {
    let Some(ip) = ip else { return Ok(HeaderMap::new()) };
    let lim = &st.0.limiter;
    let q = lim.check(ip);
    let h = lim.headers(&q);
    if q.ok {
        return Ok(h);
    }
    let msg = format!("More than {} reports from this address in {} minutes.", lim.max, lim.window.as_secs() / 60);
    let mut res = problem(StatusCode::TOO_MANY_REQUESTS, "rate_limited", &msg, "Wait the number of seconds in Retry-After, then send again.");
    res.headers_mut().extend(h);
    Err(res)
}

fn invalid_json(e: serde_json::Error) -> Response {
    problem(
        StatusCode::BAD_REQUEST,
        "invalid_json",
        &format!("The body isn't a valid report: {e}"),
        "Send the JSON object described in https://rigsweep.com/openapi.json; unknown fields are refused.",
    )
}

fn invalid_field(e: String) -> Response {
    problem(StatusCode::BAD_REQUEST, "invalid_field", &e, "Fix the named field; allowed values are in https://rigsweep.com/openapi.json.")
}

fn with(mut res: Response, h: HeaderMap) -> Response {
    res.headers_mut().extend(h);
    res
}

async fn report(State(st): State<AppState>, headers: HeaderMap, ext: Extensions, body: axum::body::Bytes) -> Response {
    let ip = client_ip(&headers, &ext);
    let rl = match limit(&st, ip) {
        Ok(h) => h,
        Err(res) => return res,
    };
    let r: Report = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return with(invalid_json(e), rl),
    };
    if let Err(e) = r.validate() {
        return with(invalid_field(e), rl);
    }
    let country = st.0.cfg.geo.country(&headers, ip);
    let res = match store::save_report(&st.0.pool, &r, &country).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fail(e),
    };
    with(res, rl)
}

async fn ext_report(State(st): State<AppState>, headers: HeaderMap, ext: Extensions, body: axum::body::Bytes) -> Response {
    let ip = client_ip(&headers, &ext);
    let rl = match limit(&st, ip) {
        Ok(h) => h,
        Err(res) => return res,
    };
    let r: ExtReport = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return with(invalid_json(e), rl),
    };
    if let Err(e) = r.validate() {
        return with(invalid_field(e), rl);
    }
    let country = st.0.cfg.geo.country(&headers, ip);
    let res = match store::save_ext_report(&st.0.pool, &r, &country).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => fail(e),
    };
    with(res, rl)
}

async fn forget(State(st): State<AppState>, Path(id): Path<String>) -> Response {
    if !install_id_ok(&id) {
        return problem(StatusCode::BAD_REQUEST, "invalid_install_id", "The install ID must be 32 lowercase hex characters.", "Use the install_id your reports were sent with.");
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
