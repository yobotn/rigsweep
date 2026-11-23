//! Settings come from the environment (on the server: /etc/yobotn/rigsweep-stats.env, loaded by pm2):
//!   DATABASE_URL     postgres://user:pass@host:25060/rigsweep?sslmode=require   (required)
//!   BIND             127.0.0.1:2000 (default; the Cloudflare tunnel connects here)
//!   SITE_DIR         folder with the static site (rigsweep.com); unset = API only
//!   ADMIN_USER       admin (default)
//!   ADMIN_PASSWORD   the dashboard is off while this is empty
//!   GEOIP_DB         path to a country .mmdb (DB-IP Lite or GeoLite2); optional
//!   TRUST_CF_COUNTRY 1 when Cloudflare is in front, to use its CF-IPCountry header
//!   RATE_MAX         reports per IP per 10 minutes (default 30)

use std::net::SocketAddr;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;

fn env(k: &str, default: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // LOG_FORMAT=json gives one JSON object per line (production, see deploy/ecosystem.config.js); plain text otherwise
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    if env("LOG_FORMAT", "") == "json" {
        tracing_subscriber::fmt().json().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
    let url = std::env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("DATABASE_URL is not set"))?;
    // connect lazily, so the site keeps serving even while the database is unreachable
    let pool = PgPoolOptions::new().max_connections(5).acquire_timeout(Duration::from_secs(5)).connect_lazy(&url)?;
    let migrate = pool.clone();
    tokio::spawn(async move {
        loop {
            match rigsweep_stats::MIGRATOR.run(&migrate).await {
                Ok(()) => break tracing::info!("database ready"),
                Err(e) => {
                    tracing::error!("database not ready, retrying in 15s: {e}");
                    tokio::time::sleep(Duration::from_secs(15)).await;
                }
            }
        }
    });
    let site_dir = std::env::var("SITE_DIR").ok().filter(|s| !s.is_empty()).map(std::path::PathBuf::from);
    let cfg = rigsweep_stats::Config {
        admin_user: env("ADMIN_USER", "admin"),
        admin_password: env("ADMIN_PASSWORD", ""),
        geo: rigsweep_stats::Geo::new(std::env::var("GEOIP_DB").ok().as_deref(), env("TRUST_CF_COUNTRY", "0") == "1")?,
        rate_max: env("RATE_MAX", "30").parse().unwrap_or(30),
        site_dir,
    };
    if cfg.admin_password.is_empty() {
        tracing::warn!("ADMIN_PASSWORD is empty: the admin dashboard is off");
    }
    let bind: SocketAddr = env("BIND", "127.0.0.1:2000").parse()?;
    let app = rigsweep_stats::router(pool, cfg);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!("listening on {bind}");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// Ctrl+C, or SIGTERM / SIGINT from pm2.
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
