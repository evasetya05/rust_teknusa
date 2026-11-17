use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sqlx::MySqlPool;
use tera::Tera;
use tokio::sync::RwLock;

use crate::config::AppConfig;

#[derive(Clone, Debug)]
pub struct CachedNarration {
    pub text: String,
    pub provider_name: String,
    pub expires_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub db: MySqlPool,
    pub tera: Arc<Tera>,
    pub config: AppConfig,
    pub http_client: reqwest::Client,
    pub narration_cache: Arc<RwLock<HashMap<String, CachedNarration>>>,
    pub provider_down: Arc<RwLock<HashMap<String, Instant>>>,
    pub rate_limiter: Arc<RwLock<HashMap<i32, (u32, Instant)>>>,
}

impl AppState {
    pub fn new(db: MySqlPool, tera: Tera, config: AppConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .user_agent("TekNusa-ERP/1.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            db,
            tera: Arc::new(tera),
            config,
            http_client,
            narration_cache: Arc::new(RwLock::new(HashMap::new())),
            provider_down: Arc::new(RwLock::new(HashMap::new())),
            rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

