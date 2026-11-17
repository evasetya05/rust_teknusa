use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use hmac::{Hmac, Mac};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::AppConfig;

type HmacSha256 = Hmac<Sha256>;

pub const MODES: &[&str] = &["off", "local", "gemini", "auto"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalProviderConfig {
    pub url: String,
    pub model: String,
    pub timeout: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeminiProviderConfig {
    pub model: String,
    pub timeout: u64,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub key_source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EffectiveLlmConfig {
    pub mode: String,
    pub local: LocalProviderConfig,
    pub gemini: GeminiProviderConfig,
    pub overridden: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct StoredOverride {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub local: Option<StoredLocal>,
    #[serde(default)]
    pub gemini: Option<StoredGemini>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct StoredLocal {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub timeout: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct StoredGemini {
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub timeout: u64,
    #[serde(default)]
    pub api_key_enc: Option<String>,
}

pub fn override_path(cfg: &AppConfig) -> PathBuf {
    cfg.private_dir.join("agent_llm.json")
}

fn derive_key(secret_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-llm|");
    hasher.update(secret_key.as_bytes());
    hasher.finalize().into()
}

pub fn encrypt_api_key(secret_key: &str, plaintext: &str) -> String {
    let key = derive_key(secret_key);
    let nonce: [u8; 16] = rand::random();
    let mut cipher = Vec::with_capacity(plaintext.len());
    for (i, &b) in plaintext.as_bytes().iter().enumerate() {
        let mut h = Sha256::new();
        h.update(key);
        h.update(nonce);
        h.update(&(i as u32).to_be_bytes());
        let mask = h.finalize()[0];
        cipher.push(b ^ mask);
    }
    let mut mac = HmacSha256::new_from_slice(&key).expect("HMAC key");
    mac.update(&nonce);
    mac.update(&cipher);
    let tag: [u8; 32] = mac.finalize().into_bytes().into();

    let mut blob = Vec::with_capacity(16 + cipher.len() + 32);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&cipher);
    blob.extend_from_slice(&tag);
    format!("v1:{}", B64URL.encode(blob))
}

pub fn decrypt_api_key(secret_key: &str, token: &str) -> Option<String> {
    let raw = token.strip_prefix("v1:")?;
    let blob = B64URL.decode(raw).ok()?;
    if blob.len() < 48 {
        return None;
    }
    let nonce = &blob[..16];
    let cipher = &blob[16..blob.len() - 32];
    let tag = &blob[blob.len() - 32..];

    let key = derive_key(secret_key);
    let mut mac = HmacSha256::new_from_slice(&key).expect("HMAC key");
    mac.update(nonce);
    mac.update(cipher);
    let expected: [u8; 32] = mac.finalize().into_bytes().into();
    if expected != tag {
        return None;
    }

    let mut plain = Vec::with_capacity(cipher.len());
    for (i, &c) in cipher.iter().enumerate() {
        let mut h = Sha256::new();
        h.update(key);
        h.update(nonce);
        h.update(&(i as u32).to_be_bytes());
        let mask = h.finalize()[0];
        plain.push(c ^ mask);
    }
    String::from_utf8(plain).ok()
}

pub fn load_override(cfg: &AppConfig) -> Option<StoredOverride> {
    let path = override_path(cfg);
    let content = fs::read_to_string(path).ok()?;
    let mut data: StoredOverride = serde_json::from_str(&content).ok()?;
    if let Some(ref mut g) = data.gemini {
        if g.model.contains("3.8") || g.model.is_empty() {
            g.model = "gemini-1.5-flash".to_string();
        }
    }
    Some(data)
}

pub fn get_stored_api_key(cfg: &AppConfig) -> String {
    let Some(ov) = load_override(cfg) else {
        return String::new();
    };
    let Some(token) = ov.gemini.and_then(|g| g.api_key_enc) else {
        return String::new();
    };
    decrypt_api_key(&cfg.secret_key, &token).unwrap_or_default()
}

pub fn key_hint(key: &str) -> String {
    if key.is_empty() {
        String::new()
    } else {
        let tail: String = key
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("…{}", tail)
    }
}

pub struct CleanedOverride {
    pub stored: StoredOverride,
    pub new_api_key: Option<String>,
    pub clear_api_key: bool,
}

pub fn clean_override(input: &HashMap<String, String>) -> Result<CleanedOverride, String> {
    let mode = input
        .get("mode")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    if !MODES.contains(&mode.as_str()) {
        return Err("Mode tidak valid.".to_string());
    }

    let url_raw = input
        .get("local_url")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://ollama:11434".to_string());
    if mode == "local" || mode == "auto" {
        let url_re = Regex::new(r"^https?://[^\s/]+(:\d+)?/?$").unwrap();
        if !url_re.is_match(&url_raw) {
            return Err("URL Ollama harus berformat http(s)://host:port.".to_string());
        }
    }

    let model_re = Regex::new(r"^[A-Za-z0-9._:\-/]{1,100}$").unwrap();
    let key_re = Regex::new(r"^[A-Za-z0-9_\-]{20,200}$").unwrap();

    let local_model = input
        .get("local_model")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "llama3.2:1b".to_string());
    let mut gemini_model = input
        .get("gemini_model")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "gemini-1.5-flash".to_string());
    if gemini_model.contains("3.8") {
        gemini_model = "gemini-1.5-flash".to_string();
    }

    if !model_re.is_match(&local_model) || !model_re.is_match(&gemini_model) {
        return Err("Nama model tidak valid.".to_string());
    }

    let local_timeout: u64 = input
        .get("local_timeout")
        .and_then(|s| if s.trim().is_empty() { None } else { s.trim().parse().ok() })
        .unwrap_or(20);
    let gemini_timeout: u64 = input
        .get("gemini_timeout")
        .and_then(|s| if s.trim().is_empty() { None } else { s.trim().parse().ok() })
        .unwrap_or(30);

    if !(1..=300).contains(&local_timeout) || !(1..=300).contains(&gemini_timeout) {
        return Err("Timeout harus antara 1 dan 300 detik.".to_string());
    }

    let raw_key = input
        .get("gemini_api_key")
        .map(|s| s.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
        .unwrap_or_default();
    let new_api_key = if !raw_key.is_empty() {
        if !key_re.is_match(&raw_key) {
            return Err(
                "Format API key Gemini tidak valid. Pastikan tidak ada spasi atau tanda kutip."
                    .to_string(),
            );
        }
        Some(raw_key)
    } else {
        None
    };

    let clear_api_key = input
        .get("clear_gemini_key")
        .map(|s| matches!(s.to_lowercase().as_str(), "1" | "on" | "true"))
        .unwrap_or(false);

    Ok(CleanedOverride {
        stored: StoredOverride {
            mode,
            local: Some(StoredLocal {
                url: url_raw.trim_end_matches('/').to_string(),
                model: local_model,
                timeout: local_timeout,
            }),
            gemini: Some(StoredGemini {
                model: gemini_model,
                timeout: gemini_timeout,
                api_key_enc: None,
            }),
        },
        new_api_key,
        clear_api_key,
    })
}

pub fn save_override(cfg: &AppConfig, mut cleaned: CleanedOverride) -> Result<(), String> {
    let existing_enc = load_override(cfg)
        .and_then(|o| o.gemini)
        .and_then(|g| g.api_key_enc);

    let enc = if let Some(new_key) = cleaned.new_api_key {
        Some(encrypt_api_key(&cfg.secret_key, &new_key))
    } else if !cleaned.clear_api_key {
        existing_enc
    } else {
        None
    };

    if let Some(ref mut g) = cleaned.stored.gemini {
        g.api_key_enc = enc;
    }

    let path = override_path(cfg);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&cleaned.stored).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_override(cfg: &AppConfig) {
    let _ = fs::remove_file(override_path(cfg));
}

pub fn get_config(cfg: &AppConfig) -> EffectiveLlmConfig {
    let ov = load_override(cfg);
    let overridden = ov.is_some();

    let mut mode = cfg.agent_llm_mode.clone();
    let mut local = LocalProviderConfig {
        url: cfg.ollama_url.trim_end_matches('/').to_string(),
        model: cfg.ollama_model.clone(),
        timeout: cfg.ollama_timeout,
    };
    let mut gemini = GeminiProviderConfig {
        model: cfg.gemini_model.clone(),
        timeout: cfg.gemini_timeout,
        api_key: cfg.gemini_api_key.clone(),
        key_source: if cfg.gemini_api_key.is_empty() {
            String::new()
        } else {
            "env".to_string()
        },
    };

    if let Some(o) = ov {
        if MODES.contains(&o.mode.as_str()) {
            mode = o.mode;
        }
        if let Some(loc) = o.local {
            if !loc.url.is_empty() {
                local.url = loc.url.trim_end_matches('/').to_string();
            }
            if !loc.model.is_empty() {
                local.model = loc.model;
            }
            if loc.timeout > 0 {
                local.timeout = loc.timeout;
            }
        }
        if let Some(gem) = o.gemini {
            if !gem.model.is_empty() {
                gemini.model = if gem.model.contains("3.8") {
                    "gemini-1.5-flash".to_string()
                } else {
                    gem.model
                };
            }
            if gem.timeout > 0 {
                gemini.timeout = gem.timeout;
            }
            if let Some(enc) = gem.api_key_enc {
                if let Some(dec) = decrypt_api_key(&cfg.secret_key, &enc) {
                    if !dec.is_empty() {
                        gemini.api_key = dec;
                        gemini.key_source = "ui".to_string();
                    }
                }
            }
        }
    }

    EffectiveLlmConfig {
        mode,
        local,
        gemini,
        overridden,
    }
}

#[derive(Clone, Debug)]
pub enum LlmProvider {
    Ollama(OllamaProvider),
    Gemini(GeminiProvider),
}

impl LlmProvider {
    pub fn name(&self) -> &'static str {
        match self {
            LlmProvider::Ollama(_) => "ollama",
            LlmProvider::Gemini(_) => "gemini",
        }
    }

    pub fn model(&self) -> &str {
        match self {
            LlmProvider::Ollama(p) => &p.model,
            LlmProvider::Gemini(p) => &p.model,
        }
    }

    pub async fn generate(&self, client: &reqwest::Client, prompt: &str) -> Result<String, String> {
        match self {
            LlmProvider::Ollama(p) => p.generate(client, prompt).await,
            LlmProvider::Gemini(p) => p.generate(client, prompt).await,
        }
    }
}

#[derive(Clone, Debug)]
pub struct OllamaProvider {
    pub url: String,
    pub model: String,
    pub timeout: u64,
    pub max_tokens: u32,
}

impl OllamaProvider {
    pub async fn generate(&self, client: &reqwest::Client, prompt: &str) -> Result<String, String> {
        let endpoint = format!("{}/api/generate", self.url.trim_end_matches('/'));
        let resp = client
            .post(&endpoint)
            .json(&serde_json::json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false,
                "options": { "temperature": 0.2, "num_predict": self.max_tokens }
            }))
            .timeout(Duration::from_secs(self.timeout))
            .send()
            .await
            .map_err(|e| format!("ollama request failed: {}", e))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("ollama invalid JSON: {}", e))?;
        let text = body["response"].as_str().unwrap_or("").trim().to_string();
        if text.is_empty() {
            return Err("ollama returned an empty response".to_string());
        }
        Ok(text)
    }
}

#[derive(Clone, Debug)]
pub struct GeminiProvider {
    pub api_key: String,
    pub model: String,
    pub timeout: u64,
    pub max_tokens: u32,
}

impl GeminiProvider {
    pub async fn generate(&self, client: &reqwest::Client, prompt: &str) -> Result<String, String> {
        if self.api_key.is_empty() {
            return Err("gemini API key is not configured (GEMINI_API_KEY)".to_string());
        }
        let mut gen_cfg = serde_json::json!({
            "temperature": 0.2,
            "maxOutputTokens": self.max_tokens,
        });
        if self.model.contains("thinking") {
            gen_cfg["thinkingConfig"] = serde_json::json!({ "thinkingBudget": 0 });
        }
        let endpoint = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model
        );
        let resp = client
            .post(&endpoint)
            .header("x-goog-api-key", &self.api_key)
            .json(&serde_json::json!({
                "contents": [{"parts": [{"text": prompt}]}],
                "generationConfig": gen_cfg,
            }))
            .timeout(Duration::from_secs(self.timeout))
            .send()
            .await
            .map_err(|e| format!("gemini request failed: {}", e))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("gemini invalid response: {}", e))?;

        if !status.is_success() {
            let msg = body["error"]["message"].as_str().unwrap_or("");
            let detail = if msg.is_empty() {
                String::new()
            } else {
                format!(": {}", msg)
            };
            return Err(format!("gemini HTTP error {}{}", status.as_u16(), detail));
        }

        let parts = body["candidates"][0]["content"]["parts"]
            .as_array()
            .ok_or_else(|| "gemini returned an unexpected response format".to_string())?;
        let text: String = parts
            .iter()
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();
        if text.is_empty() {
            return Err("gemini returned an empty response".to_string());
        }
        Ok(text)
    }
}

pub fn get_chain(cfg: &AppConfig, max_tokens: u32) -> Vec<LlmProvider> {
    let eff = get_config(cfg);
    let ollama = LlmProvider::Ollama(OllamaProvider {
        url: eff.local.url,
        model: eff.local.model,
        timeout: eff.local.timeout,
        max_tokens,
    });
    let gemini = LlmProvider::Gemini(GeminiProvider {
        api_key: eff.gemini.api_key,
        model: eff.gemini.model,
        timeout: eff.gemini.timeout,
        max_tokens,
    });

    match eff.mode.as_str() {
        "local" => vec![ollama],
        "gemini" => vec![gemini],
        "auto" => vec![ollama, gemini],
        _ => Vec::new(),
    }
}

