use std::env;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub secret_key: String,
    pub database_url: String,
    pub agent_llm_mode: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub ollama_timeout: u64,
    pub gemini_api_key: String,
    pub gemini_model: String,
    pub gemini_timeout: u64,
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
    pub media_root: PathBuf,
    pub static_root: PathBuf,
    pub private_dir: PathBuf,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let secret_key = env::var("SECRET_KEY")
            .unwrap_or_else(|_| "django-insecure-1234567890".to_string());

        let db_host = env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string());
        let db_port = env::var("DB_PORT").unwrap_or_else(|_| "5433".to_string());
        let db_name = env::var("DB_NAME").unwrap_or_else(|_| "teknusas_teknusa".to_string());
        let db_user = env::var("DB_USER").unwrap_or_else(|_| "teknusas_teknusa".to_string());
        let db_password = env::var("DB_PASSWORD").unwrap_or_else(|_| "@Pontianak123".to_string());

        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            format!(
                "postgres://{}:{}@{}:{}/{}",
                urlencoding::encode(&db_user),
                urlencoding::encode(&db_password),
                db_host,
                db_port,
                db_name
            )
        });

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8000);

        let agent_llm_mode =
            env::var("AGENT_LLM_MODE").unwrap_or_else(|_| "gemini".to_string());
        let ollama_url =
            env::var("OLLAMA_URL").unwrap_or_else(|_| "http://ollama:11434".to_string());
        let ollama_model =
            env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2:1b".to_string());
        let ollama_timeout = env::var("OLLAMA_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(20);

        let gemini_api_key = env::var("GEMINI_API_KEY").unwrap_or_default();
        let mut gemini_model =
            env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-1.5-flash".to_string());
        if gemini_model.is_empty() || gemini_model.contains("3.8") {
            gemini_model = "gemini-1.5-flash".to_string();
        }
        let gemini_timeout = env::var("GEMINI_TIMEOUT")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);

        let telegram_bot_token = env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default();
        let telegram_chat_id = env::var("TELEGRAM_CHAT_ID").unwrap_or_default();

        let media_root = PathBuf::from(
            env::var("MEDIA_ROOT").unwrap_or_else(|_| "media".to_string()),
        );
        let static_root = PathBuf::from(
            env::var("STATIC_ROOT").unwrap_or_else(|_| "static".to_string()),
        );
        let private_dir = PathBuf::from(
            env::var("PRIVATE_DIR").unwrap_or_else(|_| "private".to_string()),
        );

        Self {
            host,
            port,
            secret_key,
            database_url,
            agent_llm_mode,
            ollama_url,
            ollama_model,
            ollama_timeout,
            gemini_api_key,
            gemini_model,
            gemini_timeout,
            telegram_bot_token,
            telegram_chat_id,
            media_root,
            static_root,
            private_dir,
        }
    }
}

