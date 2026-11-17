use std::collections::HashSet;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::agent::collectors::{collect_board, collect_finance, collect_leads, resolve_entities};
use crate::agent::config::AgentThresholds;
use crate::agent::llm::{build_facts, extract_numbers};
use crate::agent::narrator::{narrate_board, narrate_finance, narrate_leads};
use crate::agent::providers::get_chain;
use crate::agent::rules::{finance_alerts, kanban_alerts, leads_alerts, money};
use crate::state::AppState;

pub const MAX_QUESTION_CHARS: usize = 500;
pub const MAX_HISTORY_TURNS: usize = 6;
pub const MAX_FACTS_CHARS: usize = 4000;
pub const MAX_ANSWER_CHARS: usize = 900;
pub const RATE_LIMIT: u32 = 20;
pub const RATE_WINDOW_SECONDS: u64 = 60;
pub const SOURCE_FALLBACK: &str = "fallback";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: String,
    pub text: String,
}

pub fn clean_history(history: &[ChatTurn]) -> Vec<ChatTurn> {
    let cleaned: Vec<ChatTurn> = history
        .iter()
        .filter(|t| (t.role == "user" || t.role == "assistant") && !t.text.trim().is_empty())
        .map(|t| ChatTurn {
            role: t.role.clone(),
            text: t.text.trim().chars().take(MAX_QUESTION_CHARS).collect(),
        })
        .collect();
    let skip = cleaned.len().saturating_sub(MAX_HISTORY_TURNS);
    cleaned.into_iter().skip(skip).collect()
}

pub fn format_history(history: &[ChatTurn]) -> String {
    if history.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = history
        .iter()
        .map(|t| {
            let prefix = if t.role == "user" {
                "User: "
            } else {
                "Assistant: "
            };
            format!("{}{}", prefix, t.text)
        })
        .collect();
    format!("HISTORY:\n{}\n\n", lines.join("\n"))
}

pub fn is_valid_answer(answer: &str, facts: &str, question: &str, history: &[ChatTurn]) -> bool {
    let trimmed = answer.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_ANSWER_CHARS {
        return false;
    }
    let mut allowed: HashSet<String> = extract_numbers(facts);
    allowed.extend(extract_numbers(question));
    for turn in history {
        if turn.role == "user" {
            allowed.extend(extract_numbers(&turn.text));
        }
    }
    extract_numbers(trimmed).is_subset(&allowed)
}

pub async fn rate_limited(state: &AppState, user_id: i32) -> bool {
    let now = Instant::now();
    let mut map = state.rate_limiter.write().await;
    let entry = map
        .entry(user_id)
        .or_insert((0, now + Duration::from_secs(RATE_WINDOW_SECONDS)));
    if entry.1 <= now {
        *entry = (1, now + Duration::from_secs(RATE_WINDOW_SECONDS));
        return false;
    }
    entry.0 += 1;
    entry.0 > RATE_LIMIT
}

pub async fn build_chat_facts(state: &AppState, session_entity_id: &str) -> String {
    let today = Utc::now().date_naive();
    let t = AgentThresholds::default();
    let entities = resolve_entities(state, session_entity_id).await;
    let mut blocks = Vec::new();

    for e in &entities {
        let fm = collect_finance(state, e, today).await;
        let fa = finance_alerts(&fm, &t);
        blocks.push(build_facts(&narrate_finance(&e.name, &fm, &fa), &fa));
        for ch in fm.expense_changes.iter().take(3) {
            blocks.push(format!(
                "- Expense {}: this month {}, last month {}.",
                ch.account,
                money(ch.current),
                money(ch.previous)
            ));
        }

        let boards = crate::db::pg_query("SELECT id, name FROM kanban_board WHERE entity_id = ? ORDER BY name ASC")
            .bind(&e.id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
        for br in boards {
            let bid: i64 = br.get("id");
            let bname: String = br.get("name");
            let bm = collect_board(state, bid, &bname, today, &t).await;
            let ba = kanban_alerts(&bm, &t);
            blocks.push(build_facts(&narrate_board(&bm, &ba), &ba));
        }
    }

    for e in &entities {
        let lm = collect_leads(state, e, today, &t).await;
        let la = leads_alerts(&lm, &t);
        blocks.push(build_facts(&narrate_leads(&e.name, &lm, &la), &la));
        for l in lm.top_leads.iter().take(2) {
            let co = if l.company.is_empty() {
                String::new()
            } else {
                format!(" ({})", l.company)
            };
            blocks.push(format!(
                "- Priority lead ({}): {}{}, status {}, value {}.",
                e.name,
                l.name,
                co,
                l.status,
                money(l.estimated_value)
            ));
        }
    }

    blocks.join("\n").chars().take(MAX_FACTS_CHARS).collect()
}

pub async fn answer_question(
    state: &AppState,
    question: &str,
    history: &[ChatTurn],
    session_entity_id: &str,
) -> serde_json::Value {
    let q_clean: String = question.trim().chars().take(MAX_QUESTION_CHARS).collect();
    let hist = clean_history(history);
    let chain = get_chain(&state.config, 400);
    let facts = build_chat_facts(state, session_entity_id).await;

    if chain.is_empty() {
        return serde_json::json!({
            "answer": format!("AI Agent is currently disabled (off mode). Enable it on the Agent AI & LLM page. Current report summary:\n{}", facts),
            "source": SOURCE_FALLBACK,
        });
    }

    let prompt = format!(
        "You are the TekNusa business report assistant. Answer user questions in English, concise (maximum 4 sentences), strictly based ONLY on the FACTS below. Do not invent, round, recalculate, or add any numbers. If the answer is not present in the facts, state that the data is not available. Output plain text only.\n\nFACTS:\n{}\n\n{}QUESTION: {}\nANSWER:",
        facts,
        format_history(&hist),
        q_clean
    );

    for provider in chain {
        match provider.generate(&state.http_client, &prompt).await {
            Ok(out) => {
                if is_valid_answer(&out, &facts, &q_clean, &hist) {
                    return serde_json::json!({
                        "answer": out,
                        "source": provider.name(),
                    });
                }
                tracing::warn!(
                    "Chat provider {} output rejected by validation",
                    provider.name()
                );
            }
            Err(err) => {
                tracing::warn!("Chat provider {} failed: {}", provider.name(), err);
            }
        }
    }

    serde_json::json!({
        "answer": format!("Sorry, I cannot answer safely right now (AI provider is unavailable or returned unverified figures). Current report summary:\n{}", facts),
        "source": SOURCE_FALLBACK,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::is_valid_output;
    use crate::agent::rules::{finance_alerts, overall_status, Alert, CRITICAL, INFO, WARNING};

    #[test]
    fn test_hallucination_guard_numbers() {
        let facts = "PT Teknusa: net profit for 2026-11 is Rp 5,000,000 (revenue Rp 12,000,000, expenses Rp 7,000,000).";
        assert!(is_valid_output(
            "Laba bersih adalah Rp 5,000,000 dari pendapatan Rp 12,000,000.",
            facts
        ));
        // Hallucinated number 9999999 must be rejected!
        assert!(!is_valid_output(
            "Laba bersih adalah Rp 9,999,999.",
            facts
        ));
        assert!(!is_valid_output("", facts));
    }

    #[test]
    fn test_chat_guard_allows_question_numbers() {
        let facts = "Board Ops: 4 open task(s), 2 completed in the last 7 days.";
        let q = "What happened in 2026?";
        assert!(is_valid_answer("In 2026, there are 4 open tasks.", facts, q, &[]));
        assert!(!is_valid_answer("There are 99 open tasks.", facts, q, &[]));
    }

    #[test]
    fn test_overall_status_and_alerts() {
        assert_eq!(overall_status(&[]), "green");
        assert_eq!(overall_status(&[Alert::new(INFO, "i", "info")]), "green");
        assert_eq!(
            overall_status(&[Alert::new(WARNING, "w", "warn")]),
            "yellow"
        );
        assert_eq!(
            overall_status(&[
                Alert::new(WARNING, "w", "warn"),
                Alert::new(CRITICAL, "c", "crit")
            ]),
            "red"
        );
        let fm = FinanceMetrics {
            month: "2026-11".into(),
            today: "2026-11-17".into(),
            days_left_in_month: 13,
            current: crate::agent::rules::PeriodTotals {
                revenue: 100.0,
                expenses: 200.0,
                net_profit: -100.0,
            },
            previous: crate::agent::rules::PeriodTotals {
                revenue: 0.0,
                expenses: 0.0,
                net_profit: 0.0,
            },
            cash: -50.0,
            open_period: "2026-11".into(),
            unposted: crate::agent::rules::UnpostedSummary {
                count: 1,
                oldest_date: None,
            },
            unbalanced: 1,
            unbalanced_items: vec![],
            expense_changes: vec![],
        };
        let alerts = finance_alerts(&fm, &AgentThresholds::default());
        assert_eq!(overall_status(&alerts), "red");
        assert!(alerts.iter().any(|a| a.code == "unbalanced"));
        assert!(alerts.iter().any(|a| a.code == "negative_cash"));
    }
}

