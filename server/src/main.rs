//! Settings come from the environment (on the server: /etc/rigsweep-stats.env, readable by root only):
//!   DATABASE_URL     postgres://user:pass@host:25060/rigsweep?sslmode=require   (required)
//!   BIND             127.0.0.1:8787 (default; Caddy in front does HTTPS)
//!   ADMIN_USER       admin (default)
//!   ADMIN_PASSWORD   the dashboard is off while this is empty
//!   GEOIP_DB         path to a country .mmdb (DB-IP Lite or GeoLite2); optional
//!   TRUST_CF_COUNTRY 1 when Cloudflare is in front, to use its CF-IPCountry header
//!   RATE_MAX         reports per IP per 10 minutes (default 30)

use std::net::SocketAddr;

use sqlx::postgres::PgPoolOptions;

fn env(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())).init();
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is not set"))?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await?;
    rigsweep_stats::MIGRATOR.run(&pool).await?;
    let cfg = rigsweep_stats::Config {
        admin_user: env("ADMIN_USER", "admin"),
        admin_password: env("ADMIN_PASSWORD", ""),
        geo: rigsweep_stats::Geo::new(std::env::var("GEOIP_DB").ok().as_deref(), env("TRUST_CF_COUNTRY", "0") == "1")?,
        rate_max: env("RATE_MAX", "30").parse().unwrap_or(30),
    };
    if cfg.admin_password.is_empty() {
        tracing::warn!("ADMIN_PASSWORD is empty: the admin dashboard is off");
    }
    let bind: SocketAddr = env("BIND", "127.0.0.1:8787").parse()?;
    let app = rigsweep_stats::router(pool, cfg);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("listening on {bind}");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// Ctrl+C, or SIGTERM from systemd.
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
