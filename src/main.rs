mod agent;
mod auth;
mod config;
mod db;
mod models;
mod routes;
mod state;

use std::collections::HashMap;
use std::net::SocketAddr;

use serde_json::Value;
use tera::Tera;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::agent::rules::format_comma_i64;
use crate::config::AppConfig;
use crate::state::AppState;

fn intcomma_filter(value: &Value, _args: &HashMap<String, Value>) -> tera::Result<Value> {
    let num = match value {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i
            } else if let Some(u) = n.as_u64() {
                u as i64
            } else if let Some(f) = n.as_f64() {
                f.round() as i64
            } else {
                0
            }
        }
        Value::String(s) => s.trim().parse::<f64>().map(|f| f.round() as i64).unwrap_or(0),
        _ => 0,
    };
    Ok(Value::String(format_comma_i64(num)))
}

fn build_tera() -> anyhow::Result<Tera> {
    let mut tera = Tera::new("templates/**/*.html")?;
    tera.register_filter("intcomma", intcomma_filter);
    Ok(tera)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "erp_rust=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = AppConfig::from_env();
    let _ = tokio::fs::create_dir_all(&cfg.media_root).await;
    let _ = tokio::fs::create_dir_all(&cfg.static_root).await;
    let _ = tokio::fs::create_dir_all(&cfg.private_dir).await;

    let pool = db::connect_and_migrate(&cfg.database_url).await?;
    let tera = build_tera()?;
    let state = AppState::new(pool, tera, cfg.clone());

    let app = routes::build_router()
        .nest_service("/static", ServeDir::new(&cfg.static_root))
        .nest_service("/media", ServeDir::new(&cfg.media_root))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port).parse()?;
    tracing::info!("Teknusa Rust ERP listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tera_templates_parse_cleanly() {
        let tera = build_tera().expect("all Tera templates should parse without syntax errors");
        assert!(tera.get_template_names().count() > 20);
    }

    #[test]
    fn test_intcomma_filter() {
        let args = HashMap::new();
        let out = intcomma_filter(&serde_json::json!(1250000), &args).unwrap();
        assert_eq!(out, serde_json::json!("1,250,000"));
        let neg = intcomma_filter(&serde_json::json!(-5000), &args).unwrap();
        assert_eq!(neg, serde_json::json!("-5,000"));
    }
}
