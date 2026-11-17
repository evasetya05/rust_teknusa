use std::collections::HashMap;
use std::time::Instant;

use axum::extract::{RawForm, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::Deserialize;
use tera::Context;

use crate::agent::chat::{
    answer_question, is_valid_answer, rate_limited, ChatTurn, MAX_QUESTION_CHARS,
};
use crate::agent::collectors::build_briefings;
use crate::agent::config::AgentThresholds;
use crate::agent::llm::is_valid_output;
use crate::agent::narrator::{narrate_board, narrate_finance, narrate_leads};
use crate::agent::providers::{
    clean_override, clear_override, get_chain, get_config, get_stored_api_key, key_hint,
    load_override, save_override, GeminiProvider, LlmProvider, OllamaProvider, MODES,
};
use crate::agent::rules::{
    finance_alerts, overall_status, Alert, FinanceMetrics, PeriodTotals, UnpostedSummary,
    CRITICAL, INFO, WARNING,
};
use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: Option<serde_json::Value>,
    pub history: Option<Vec<ChatTurn>>,
}

pub async fn briefing_view(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_admin_or_403() {
        return resp;
    }
    let briefings = build_briefings(&state, "all").await;
    let mut ctx = Context::new();
    ctx.insert("finance", &briefings.finance);
    ctx.insert("kanban", &briefings.kanban);
    ctx.insert("kanban_by_entity", &briefings.kanban_by_entity);
    ctx.insert("leads", &briefings.leads);
    render_page(&state, &session, "agent/briefing.html", ctx)
}

pub async fn llm_status_view(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_admin_or_403() {
        return resp;
    }
    let cfg = get_config(&state.config);
    let chain = get_chain(&state.config, 160);
    let chain_names: Vec<&str> = chain.iter().map(|p| p.name()).collect();
    let stored_key = get_stored_api_key(&state.config);

    let mut ctx = Context::new();
    ctx.insert("mode", &cfg.mode);
    ctx.insert("local", &cfg.local);
    ctx.insert("gemini", &cfg.gemini);
    ctx.insert("gemini_key_set", &(!cfg.gemini.api_key.is_empty()));
    ctx.insert("gemini_key_source", &cfg.gemini.key_source);
    ctx.insert("gemini_key_hint", &key_hint(&stored_key));
    ctx.insert("modes", &MODES);
    ctx.insert("overridden", &load_override(&state.config).is_some());
    ctx.insert("enabled", &(!chain_names.is_empty()));
    ctx.insert("chain", &chain_names);
    render_page(&state, &session, "agent/llm_status.html", ctx)
}

pub async fn llm_settings_view(
    State(state): State<AppState>,
    mut session: SessionContext,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_admin_or_403() {
        return resp;
    }

    let form = parse_url_form(&raw);
    if form.get("action").map(|s| s.as_str()) == Some("reset") {
        clear_override(&state.config);
        session.push_flash(
            "success",
            "Pengaturan manual dihapus; kembali ke konfigurasi environment.",
        );
        return redirect_with_session(&state, &session.payload, "/agent/llm/");
    }

    match clean_override(&form) {
        Ok(cleaned) => {
            let mode = cleaned.stored.mode.clone();
            match save_override(&state.config, cleaned) {
                Ok(_) => {
                    let cfg = get_config(&state.config);
                    if (mode == "gemini" || mode == "auto") && cfg.gemini.api_key.is_empty() {
                        session.push_flash(
                            "warning",
                            format!(
                                "Pengaturan disimpan (mode {}), tetapi API Key Gemini belum terpasang.",
                                mode.to_uppercase()
                            ),
                        );
                    } else {
                        let mode_display = match mode.as_str() {
                            "off" => "Off (Template)",
                            "local" => "Lokal (Ollama)",
                            "gemini" => "API (Gemini)",
                            "auto" => "Auto (Ollama lalu Gemini)",
                            _ => mode.as_str(),
                        };
                        session.push_flash(
                            "success",
                            format!(
                                "Pengaturan agent AI berhasil disimpan. Mode aktif: {}.",
                                mode_display
                            ),
                        );
                    }
                }
                Err(e) => {
                    session.push_flash("error", format!("Gagal menyimpan pengaturan: {}", e));
                }
            }
        }
        Err(msg) => {
            session.push_flash("error", msg);
        }
    }

    redirect_with_session(&state, &session.payload, "/agent/llm/")
}

pub async fn llm_test_view(
    State(state): State<AppState>,
    session: SessionContext,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_admin_or_403() {
        return resp;
    }

    let form = parse_url_form(&raw);
    let target = form
        .get("provider")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    let cfg = get_config(&state.config);

    let chain: Vec<LlmProvider> = if target == "gemini" {
        let input_key = form
            .get("gemini_api_key")
            .map(|s| s.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
            .unwrap_or_default();
        let api_key = if !input_key.is_empty() {
            input_key
        } else {
            cfg.gemini.api_key.clone()
        };
        if api_key.is_empty() {
            return Json(serde_json::json!({
                "ok": false,
                "error": "API Key Gemini belum diisi. Masukkan API key pada kolom Gemini lalu klik Simpan atau Test.",
            }))
            .into_response();
        }
        let mut model = form
            .get("gemini_model")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| cfg.gemini.model.clone());
        if model.contains("3.8") {
            model = "gemini-1.5-flash".to_string();
        }
        let timeout = form
            .get("gemini_timeout")
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(cfg.gemini.timeout);
        vec![LlmProvider::Gemini(GeminiProvider {
            api_key,
            model,
            timeout,
            max_tokens: 400,
        })]
    } else if target == "local" {
        let url = form
            .get("local_url")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| cfg.local.url.clone());
        let model = form
            .get("local_model")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| cfg.local.model.clone());
        let timeout = form
            .get("local_timeout")
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(cfg.local.timeout);
        vec![LlmProvider::Ollama(OllamaProvider {
            url,
            model,
            timeout,
            max_tokens: 160,
        })]
    } else {
        let c = get_chain(&state.config, 160);
        if c.is_empty() {
            return Json(serde_json::json!({
                "ok": false,
                "error": "LLM mode saat ini 'off'. Pilih mode lain di form (Lokal, Gemini, atau Auto) lalu Simpan.",
            }))
            .into_response();
        }
        c
    };

    let mut results = Vec::new();
    let mut all_ok = true;
    for provider in chain {
        let started = Instant::now();
        match provider
            .generate(&state.http_client, "Reply with the single word: OK")
            .await
        {
            Ok(text) => {
                let snippet: String = text.chars().take(200).collect();
                results.push(serde_json::json!({
                    "provider": provider.name(),
                    "model": provider.model(),
                    "ok": true,
                    "response": snippet,
                    "seconds": (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
                }));
            }
            Err(err) => {
                all_ok = false;
                results.push(serde_json::json!({
                    "provider": provider.name(),
                    "model": provider.model(),
                    "ok": false,
                    "error": err,
                    "seconds": (started.elapsed().as_secs_f64() * 100.0).round() / 100.0,
                }));
            }
        }
    }

    Json(serde_json::json!({ "ok": all_ok, "results": results })).into_response()
}

pub async fn llm_test_suite_view(session: SessionContext) -> Response {
    if let Err(resp) = session.require_admin_or_403() {
        return resp;
    }

    let started = Instant::now();
    let mut log_lines = Vec::new();
    let mut failures = 0usize;
    let mut run_case = |name: &str, ok: bool| {
        if ok {
            log_lines.push(format!("{} ... ok", name));
        } else {
            failures += 1;
            log_lines.push(format!("{} ... FAIL", name));
        }
    };

    // 1. Validation accepts valid numbers
    let facts = "PT Teknusa: net profit for 2026-11 is Rp 5,000,000 (revenue Rp 12,000,000, expenses Rp 7,000,000).";
    run_case(
        "test_valid_output_accepts_known_numbers",
        is_valid_output("Laba bersih Rp 5,000,000 dari Rp 12,000,000.", facts),
    );

    // 2. Validation rejects invented numbers
    run_case(
        "test_valid_output_rejects_hallucinated_number",
        !is_valid_output("Laba bersih Rp 8,888,888.", facts),
    );

    // 3. Validation rejects empty/too-long output
    let too_long = "a".repeat(650);
    run_case(
        "test_valid_output_rejects_empty_and_overlength",
        !is_valid_output("", facts) && !is_valid_output(&too_long, facts),
    );

    // 4. Chat validation allows numbers from user question
    run_case(
        "test_chat_validation_allows_question_numbers",
        is_valid_answer(
            "In 2026, revenue is Rp 12,000,000.",
            facts,
            "What is revenue in 2026?",
            &[],
        ) && !is_valid_answer(
            "Revenue is Rp 99,000,000.",
            facts,
            "What is revenue in 2026?",
            &[],
        ),
    );

    // 5. Alert rules & overall status
    let fm = FinanceMetrics {
        month: "2026-11".into(),
        today: "2026-11-17".into(),
        days_left_in_month: 13,
        current: PeriodTotals {
            revenue: 100.0,
            expenses: 200.0,
            net_profit: -100.0,
        },
        previous: PeriodTotals {
            revenue: 500.0,
            expenses: 100.0,
            net_profit: 400.0,
        },
        cash: -10.0,
        open_period: "2026-11".into(),
        unposted: UnpostedSummary {
            count: 2,
            oldest_date: None,
        },
        unbalanced: 1,
        unbalanced_items: vec![],
        expense_changes: vec![],
    };
    let alerts = finance_alerts(&fm, &AgentThresholds::default());
    run_case(
        "test_finance_rules_detect_unbalanced_and_negative_cash",
        overall_status(&alerts) == "red"
            && alerts.iter().any(|a| a.code == "unbalanced")
            && alerts.iter().any(|a| a.code == "negative_cash"),
    );

    // 6. Status severity mapping
    run_case(
        "test_overall_status_traffic_lights",
        overall_status(&[]) == "green"
            && overall_status(&[Alert::new(INFO, "x", "x")]) == "green"
            && overall_status(&[Alert::new(WARNING, "y", "y")]) == "yellow"
            && overall_status(&[Alert::new(CRITICAL, "z", "z")]) == "red",
    );

    // 7. Deterministic narrator
    let fin_nar = narrate_finance("PT Teknusa", &fm, &alerts);
    run_case(
        "test_narrator_finance_contains_entity_and_month",
        fin_nar.contains("PT Teknusa") && fin_nar.contains("2026-11"),
    );

    let bm = crate::agent::rules::KanbanBoardMetrics {
        board_id: 1,
        board: "Ops".into(),
        open_total: 3,
        overdue: 0,
        urgent_overdue: 0,
        due_soon: 1,
        unassigned: 0,
        stalled: 0,
        completed_week: 2,
        columns: vec![],
        top_tasks: vec![],
        overdue_tasks: vec![],
    };
    let lm = crate::agent::rules::LeadsMetrics {
        entity_id: "e1".into(),
        entity_name: "PT Teknusa".into(),
        open_total: 2,
        pipeline_value: 5_000_000.0,
        overdue: 0,
        urgent_overdue: 0,
        due_today: 0,
        due_soon: 0,
        hot_count: 1,
        unassigned: 0,
        stalled: 0,
        stalled_hot: 0,
        new_month: 1,
        won_month: 0,
        top_leads: vec![],
        overdue_leads: vec![],
        stalled_leads: vec![],
        unassigned_leads: vec![],
    };
    run_case(
        "test_narrators_board_and_leads",
        narrate_board(&bm, &[]).contains("Board Ops")
            && narrate_leads("PT Teknusa", &lm, &[]).contains("Pipeline is on track."),
    );

    let duration = (started.elapsed().as_secs_f64() * 100.0).round() / 100.0;
    log_lines.push("----------------------------------------------------------------------".into());
    log_lines.push(format!("Ran 8 tests in {:.3}s", started.elapsed().as_secs_f64()));
    log_lines.push(if failures == 0 { "OK".into() } else { "FAILED".into() });

    Json(serde_json::json!({
        "ok": failures == 0,
        "tests_run": 8,
        "errors": 0,
        "failures": failures,
        "duration": duration,
        "output": log_lines.join("\n"),
    }))
    .into_response()
}

pub async fn chat_view(
    State(state): State<AppState>,
    session: SessionContext,
    body: axum::body::Bytes,
) -> Response {
    let user = match session.user.as_ref() {
        Some(u) if u.is_admin() => u.clone(),
        Some(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "ok": false,
                    "error": "Access denied: report chat is restricted to administrators."
                })),
            )
                .into_response();
        }
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "ok": false, "error": "Authentication required." })),
            )
                .into_response();
        }
    };

    let payload: ChatRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": "Invalid request." })),
            )
                .into_response();
        }
    };

    let msg = match payload.message.as_ref().and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": "Empty question." })),
            )
                .into_response();
        }
    };

    if msg.chars().count() > MAX_QUESTION_CHARS {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": "Question is too long." })),
        )
            .into_response();
    }

    if rate_limited(&state, user.id).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "ok": false,
                "error": "Too many requests. Please try again in a moment."
            })),
        )
            .into_response();
    }

    let history = payload.history.unwrap_or_default();
    let result = answer_question(&state, msg, &history, &session.current_entity_id).await;
    Json(serde_json::json!({
        "ok": true,
        "answer": result["answer"],
        "source": result["source"],
    }))
    .into_response()
}

fn parse_url_form(raw: &[u8]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in String::from_utf8_lossy(raw).split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let kd = urlencoding::decode(&k.replace('+', " "))
            .unwrap_or_else(|_| k.into())
            .into_owned();
        let vd = urlencoding::decode(&v.replace('+', " "))
            .unwrap_or_else(|_| v.into())
            .into_owned();
        map.insert(kd, vd);
    }
    map
}

