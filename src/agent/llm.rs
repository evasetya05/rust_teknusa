use std::collections::HashSet;
use std::time::{Duration, Instant};

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::agent::providers::get_chain;
use crate::agent::rules::Alert;
use crate::state::{AppState, CachedNarration};

pub const TEMPLATE_SOURCE: &str = "template";
pub const CACHE_SECONDS: u64 = 6 * 60 * 60;
pub const PROVIDER_DOWN_SECONDS: u64 = 60;
pub const MAX_OUTPUT_CHARS: usize = 600;

pub fn extract_numbers(text: &str) -> HashSet<String> {
    let re = Regex::new(r"\d[\d.,]*").unwrap();
    re.find_iter(text)
        .map(|m| m.as_str().replace(['.', ','], ""))
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn build_facts(template_text: &str, alerts: &[Alert]) -> String {
    let mut lines = vec![template_text.to_string()];
    for a in alerts {
        lines.push(format!("- {}", a.message));
    }
    lines.join("\n")
}

pub fn is_valid_output(output: &str, facts: &str) -> bool {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_OUTPUT_CHARS {
        return false;
    }
    extract_numbers(trimmed).is_subset(&extract_numbers(facts))
}

pub async fn narrate(state: &AppState, template_text: &str, alerts: &[Alert]) -> (String, String) {
    let chain = get_chain(&state.config, 160);
    if chain.is_empty() {
        return (template_text.to_string(), TEMPLATE_SOURCE.to_string());
    }

    let facts = build_facts(template_text, alerts);
    let prompt = format!(
        "Anda asisten bisnis yang ringkas. Tulis ulang FAKTA di bawah sebagai laporan status singkat dalam bahasa Indonesia, maksimal 3 kalimat. Gunakan HANYA fakta yang diberikan. Jangan mengarang, membulatkan, atau menambah angka apa pun. Jangan menghitung ulang. Jangan memberi saran yang tidak tersirat dari fakta. Keluarkan teks biasa saja.\n\nFAKTA:\n{}\n",
        facts
    );

    let now = Instant::now();
    for provider in chain {
        let mut h = Sha256::new();
        h.update(format!("{}|{}|{}", provider.name(), provider.model(), prompt).as_bytes());
        let cache_key = format!("agent:narr:{:x}", h.finalize());

        {
            let cache = state.narration_cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                if entry.expires_at > now {
                    return (entry.text.clone(), entry.provider_name.clone());
                }
            }
        }
        {
            let down = state.provider_down.read().await;
            if let Some(&until) = down.get(provider.name()) {
                if until > now {
                    continue;
                }
            }
        }

        match provider.generate(&state.http_client, &prompt).await {
            Ok(output) => {
                if is_valid_output(&output, &facts) {
                    let mut cache = state.narration_cache.write().await;
                    cache.insert(
                        cache_key,
                        CachedNarration {
                            text: output.clone(),
                            provider_name: provider.name().to_string(),
                            expires_at: now + Duration::from_secs(CACHE_SECONDS),
                        },
                    );
                    return (output, provider.name().to_string());
                }
                tracing::warn!(
                    "Narration provider {} output rejected by validation",
                    provider.name()
                );
            }
            Err(err) => {
                tracing::warn!("Narration provider {} failed: {}", provider.name(), err);
                let mut down = state.provider_down.write().await;
                down.insert(
                    provider.name().to_string(),
                    now + Duration::from_secs(PROVIDER_DOWN_SECONDS),
                );
            }
        }
    }

    (template_text.to_string(), TEMPLATE_SOURCE.to_string())
}

