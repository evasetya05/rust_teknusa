use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path as AxumPath, Query, RawForm, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::{Datelike, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tera::Context;

use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::models::{normalize_uuid, OptionItem};
use crate::state::AppState;

#[derive(Clone, Debug, Serialize)]
pub struct AccountView {
    pub id: i64,
    pub entity_id: Option<String>,
    pub account_type: String,
    pub account_name: String,
    pub coa: String,
    pub balance_type: String,
    pub active: bool,
    pub locked: bool,
    pub coa_role_default: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DimensionItemView {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub extra: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct JournalItemView {
    pub id: i64,
    pub account_id: i64,
    pub account_name: String,
    pub account_type: String,
    pub coa: String,
    pub cost_center_id: Option<i64>,
    pub cost_center_name: String,
    pub profit_center_id: Option<i64>,
    pub profit_center_name: String,
    pub project_id: Option<i64>,
    pub project_name: String,
    pub activity_id: Option<i64>,
    pub activity_name: String,
    pub location_id: Option<i64>,
    pub location_name: String,
    pub partner_id: Option<i64>,
    pub partner_name: String,
    pub product_id: Option<i64>,
    pub product_name: String,
    pub debit: i64,
    pub credit: i64,
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct JournalEntryView {
    pub id: i64,
    pub entity_id: Option<String>,
    pub date: String,
    pub description: String,
    pub period: String,
    pub is_posted: bool,
    pub items: Vec<JournalItemView>,
    pub total_debit: i64,
    pub total_credit: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClosingPeriodView {
    pub id: i64,
    pub period: String,
    pub is_closed: bool,
    pub closed_at: String,
    pub closed_by: String,
}

#[derive(Deserialize)]
pub struct AccountCreateForm {
    #[serde(default)]
    pub entity: String,
    pub account_type: String,
    pub account_name: String,
    #[serde(default)]
    pub coa: String,
    pub balance_type: String,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub locked: Option<String>,
    #[serde(default)]
    pub coa_role_default: String,
}

#[derive(Deserialize)]
pub struct ModeFilterQuery {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub year: String,
}

#[derive(Deserialize)]
pub struct DimensionReportQuery {
    #[serde(default)]
    pub dimension: String,
    #[serde(default)]
    pub dimension_id: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub start_date: String,
    #[serde(default)]
    pub end_date: String,
    #[serde(default)]
    pub account_type: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub view: String,
}

#[derive(Deserialize)]
pub struct CreateDimensionForm {
    pub dim_type: String,
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub extra_type: String,
}

pub async fn ledger_index(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    render_page(&state, &session, "ledger/index.html", Context::new())
}

pub async fn account_list(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let accounts = fetch_accounts(&state, eid.as_deref(), false).await;
    let mut ctx = Context::new();
    ctx.insert("accounts", &accounts);
    render_page(&state, &session, "ledger/account_list.html", ctx)
}

pub async fn account_create_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let mut ctx = Context::new();
    ctx.insert(
        "default_entity_id",
        &session.entity_filter_id().unwrap_or_default(),
    );
    ctx.insert(
        "account_types",
        &["ASSET", "LIABILITY", "CAPITAL", "INCOME", "COGS", "EXPENSES"],
    );
    ctx.insert("balance_types", &["Debit", "Credit"]);
    render_page(&state, &session, "ledger/account_form.html", ctx)
}

pub async fn account_create_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<AccountCreateForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let entity_id = if form.entity.trim().is_empty() {
        session.entity_filter_id()
    } else {
        Some(normalize_uuid(&form.entity))
    };
    let coa = if form.coa.trim().is_empty() {
        "Default CoA"
    } else {
        form.coa.trim()
    };
    let role = if form.coa_role_default.trim().is_empty() {
        None
    } else {
        Some(form.coa_role_default.trim().to_string())
    };
    let _ = sqlx::query(
        "INSERT INTO ledger_account (entity_id, account_type, account_name, coa, balance_type, active, locked, coa_role_default)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entity_id)
    .bind(form.account_type.trim())
    .bind(form.account_name.trim())
    .bind(coa)
    .bind(form.balance_type.trim())
    .bind(form.active.is_some())
    .bind(form.locked.is_some())
    .bind(role)
    .execute(&state.db)
    .await;

    session.push_flash("success", "Akun berhasil dibuat!");
    redirect_with_session(&state, &session.payload, "/ledger/accounts/accounts/")
}

pub async fn create_journal_entry_get(
    State(state): State<AppState>,
    session: SessionContext,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mut ctx = Context::new();
    ctx.insert("today", &Utc::now().format("%Y-%m-%d").to_string());
    ctx.insert("accounts", &fetch_accounts(&state, eid.as_deref(), false).await);
    insert_dimension_lists(&state, eid.as_deref(), &mut ctx).await;
    render_page(&state, &session, "ledger/journal_entry.html", ctx)
}

pub async fn create_journal_entry_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let form = parse_multi_form(&raw);
    let date_str = first_form_val(&form, "date");
    let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .unwrap_or_else(|_| Utc::now().date_naive());
    let description = first_form_val(&form, "description");

    let open_period = get_or_create_open_period(&state, eid.as_deref()).await;
    let date_period = date.format("%Y-%m").to_string();
    if date_period != open_period {
        session.push_flash(
            "warning",
            format!(
                "Tanggal jurnal {} berbeda dari periode open ({}). Transaksi dimasukkan ke periode {}.",
                date_period, open_period, open_period
            ),
        );
    }

    let res = sqlx::query(
        "INSERT INTO ledger_journalentry (entity_id, date, description, created_at, period, is_posted)
         VALUES (?, ?, ?, NOW(6), ?, 0)",
    )
    .bind(eid.as_deref())
    .bind(date)
    .bind(&description)
    .bind(&open_period)
    .execute(&state.db)
    .await;

    if let Ok(ok) = res {
        let je_id = ok.last_insert_id() as i64;
        insert_journal_items(&state, je_id, &form).await;
        session.push_flash(
            "success",
            format!("Jurnal berhasil dibuat untuk periode {}.", open_period),
        );
    } else {
        session.push_flash("error", "Gagal menyimpan jurnal.");
    }

    redirect_with_session(&state, &session.payload, "/ledger/journal/journal/list/")
}

pub async fn journal_list(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mut sql = String::from(
        "SELECT id, entity_id, date, description, period, is_posted
         FROM ledger_journalentry WHERE 1=1",
    );
    if eid.is_some() {
        sql.push_str(" AND entity_id = ?");
    }
    sql.push_str(" ORDER BY period DESC, date DESC, id DESC");

    let mut q = sqlx::query(&sql);
    if let Some(ref e) = eid {
        q = q.bind(e);
    }
    let rows = q.fetch_all(&state.db).await.unwrap_or_default();

    let mut grouped: BTreeMap<String, Vec<JournalEntryView>> = BTreeMap::new();
    let mut period_order: Vec<String> = Vec::new();
    for r in rows {
        let je_id: i64 = r.get("id");
        let period: Option<String> = r.get("period");
        let pkey = period
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Tanpa Periode".to_string());
        if !period_order.contains(&pkey) {
            period_order.push(pkey.clone());
        }
        let items = fetch_journal_items(&state, je_id).await;
        let total_debit: i64 = items.iter().map(|i| i.debit).sum();
        let total_credit: i64 = items.iter().map(|i| i.credit).sum();
        let dt: NaiveDate = r.get("date");
        grouped.entry(pkey).or_default().push(JournalEntryView {
            id: je_id,
            entity_id: r.get("entity_id"),
            date: dt.format("%Y-%m-%d").to_string(),
            description: r.get("description"),
            period: r.get::<Option<String>, _>("period").unwrap_or_default(),
            is_posted: r.get("is_posted"),
            items,
            total_debit,
            total_credit,
        });
    }

    let grouped_list: Vec<serde_json::Value> = period_order
        .into_iter()
        .map(|p| {
            let entries = grouped.remove(&p).unwrap_or_default();
            serde_json::json!({ "period": p, "journals": entries })
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("grouped_journals", &grouped_list);
    render_page(&state, &session, "ledger/journal_list.html", ctx)
}

pub async fn journal_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = sqlx::query(
        "SELECT id, entity_id, date, description, period, is_posted FROM ledger_journalentry WHERE id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(r) = row else {
        return Redirect::to("/ledger/journal/journal/list/").into_response();
    };
    let eid: Option<String> = r.get("entity_id");
    let items = fetch_journal_items(&state, pk).await;
    let total_debit: i64 = items.iter().map(|i| i.debit).sum();
    let total_credit: i64 = items.iter().map(|i| i.credit).sum();
    let dt: NaiveDate = r.get("date");

    let journal = JournalEntryView {
        id: pk,
        entity_id: eid.clone(),
        date: dt.format("%Y-%m-%d").to_string(),
        description: r.get("description"),
        period: r.get::<Option<String>, _>("period").unwrap_or_default(),
        is_posted: r.get("is_posted"),
        items: items.clone(),
        total_debit,
        total_credit,
    };

    let mut ctx = Context::new();
    ctx.insert("journal", &journal);
    ctx.insert("journal_items", &items);
    ctx.insert("accounts", &fetch_accounts(&state, eid.as_deref(), false).await);
    insert_dimension_lists(&state, eid.as_deref(), &mut ctx).await;
    render_page(&state, &session, "ledger/journal_edit.html", ctx)
}

pub async fn journal_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let form = parse_multi_form(&raw);
    let description = first_form_val(&form, "description");
    let is_posted = first_form_val(&form, "post") == "1";

    let _ = sqlx::query("UPDATE ledger_journalentry SET description = ?, is_posted = ? WHERE id = ?")
        .bind(&description)
        .bind(is_posted)
        .bind(pk)
        .execute(&state.db)
        .await;

    let _ = sqlx::query("DELETE FROM ledger_journalitem WHERE journal_entry_id = ?")
        .bind(pk)
        .execute(&state.db)
        .await;

    insert_journal_items(&state, pk, &form).await;
    session.push_flash("success", "Perubahan jurnal berhasil disimpan.");
    redirect_with_session(&state, &session.payload, "/ledger/journal/journal/list/")
}

pub async fn closing_period_list(
    State(state): State<AppState>,
    mut session: SessionContext,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let periods = fetch_closing_periods(&state, eid.as_deref(), false).await;

    if periods.is_empty() {
        let cur = Utc::now().format("%Y-%m").to_string();
        ensure_period_exists(&state, &cur, eid.as_deref()).await;
        session.push_flash(
            "info",
            format!("Periode awal {} dibuat otomatis.", cur),
        );
        return redirect_with_session(
            &state,
            &session.payload,
            "/ledger/closing/closing-periods/",
        );
    }

    if !periods.iter().any(|p| !p.is_closed) {
        let cur = Utc::now().format("%Y-%m").to_string();
        ensure_period_exists(&state, &cur, eid.as_deref()).await;
        if let Some(ref e) = eid {
            let _ = sqlx::query(
                "UPDATE ledger_closingperiod SET is_closed = 0 WHERE period = ? AND entity_id = ?",
            )
            .bind(&cur)
            .bind(e)
            .execute(&state.db)
            .await;
        } else {
            let _ = sqlx::query(
                "UPDATE ledger_closingperiod SET is_closed = 0 WHERE period = ? AND entity_id IS NULL",
            )
            .bind(&cur)
            .execute(&state.db)
            .await;
        }
        return redirect_with_session(
            &state,
            &session.payload,
            "/ledger/closing/closing-periods/",
        );
    }

    let mut ctx = Context::new();
    ctx.insert("periods", &periods);
    render_page(&state, &session, "ledger/closing_period_list.html", ctx)
}

pub async fn close_period(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(period): AxumPath<String>,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    let eid = session.entity_filter_id();
    ensure_period_exists(&state, &period, eid.as_deref()).await;

    let is_closed: Option<bool> = if let Some(ref e) = eid {
        sqlx::query_scalar(
            "SELECT is_closed FROM ledger_closingperiod WHERE period = ? AND entity_id = ? LIMIT 1",
        )
        .bind(&period)
        .bind(e)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_scalar(
            "SELECT is_closed FROM ledger_closingperiod WHERE period = ? AND entity_id IS NULL LIMIT 1",
        )
        .bind(&period)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
    };

    if is_closed == Some(true) {
        session.push_flash("warning", format!("Periode {} sudah tertutup.", period));
        return redirect_with_session(
            &state,
            &session.payload,
            "/ledger/closing/closing-periods/",
        );
    }

    // Post all unposted journals into this period
    if let Some(ref e) = eid {
        let _ = sqlx::query(
            "UPDATE ledger_journalentry SET period = ?, is_posted = 1 WHERE is_posted = 0 AND entity_id = ?",
        )
        .bind(&period)
        .bind(e)
        .execute(&state.db)
        .await;
        let _ = sqlx::query(
            "UPDATE ledger_closingperiod SET is_closed = 1, closed_at = NOW(6), closed_by = ? WHERE period = ? AND entity_id = ?",
        )
        .bind(&user.username)
        .bind(&period)
        .bind(e)
        .execute(&state.db)
        .await;
    } else {
        let _ = sqlx::query(
            "UPDATE ledger_journalentry SET period = ?, is_posted = 1 WHERE is_posted = 0",
        )
        .bind(&period)
        .execute(&state.db)
        .await;
        let _ = sqlx::query(
            "UPDATE ledger_closingperiod SET is_closed = 1, closed_at = NOW(6), closed_by = ? WHERE period = ? AND entity_id IS NULL",
        )
        .bind(&user.username)
        .bind(&period)
        .execute(&state.db)
        .await;
    }

    adjust_retained_earnings(&state, &mut session, &period, eid.as_deref()).await;

    if let Some(next_month) = next_month_period(&period) {
        ensure_period_exists(&state, &next_month, eid.as_deref()).await;
        if let Some(ref e) = eid {
            let _ = sqlx::query(
                "UPDATE ledger_closingperiod SET is_closed = 0, closed_at = NULL, closed_by = NULL WHERE period = ? AND entity_id = ?",
            )
            .bind(&next_month)
            .bind(e)
            .execute(&state.db)
            .await;
        } else {
            let _ = sqlx::query(
                "UPDATE ledger_closingperiod SET is_closed = 0, closed_at = NULL, closed_by = NULL WHERE period = ? AND entity_id IS NULL",
            )
            .bind(&next_month)
            .execute(&state.db)
            .await;
        }
        session.push_flash(
            "success",
            format!("Periode {} ditutup, periode {} dibuka otomatis.", period, next_month),
        );
    } else {
        session.push_flash("error", "Format periode tidak valid. Gunakan YYYY-MM.");
    }

    redirect_with_session(
        &state,
        &session.payload,
        "/ledger/closing/closing-periods/",
    )
}

async fn adjust_retained_earnings(
    state: &AppState,
    session: &mut SessionContext,
    period: &str,
    eid: Option<&str>,
) {
    let mut inc_sql = String::from(
        "SELECT COALESCE(SUM(ji.credit), 0) - COALESCE(SUM(ji.debit), 0) AS total
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE a.account_type = 'INCOME' AND je.period = ?",
    );
    let mut exp_sql = String::from(
        "SELECT COALESCE(SUM(ji.debit), 0) - COALESCE(SUM(ji.credit), 0) AS total
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE a.account_type = 'EXPENSES' AND je.period = ?",
    );
    if eid.is_some() {
        inc_sql.push_str(" AND je.entity_id = ? AND a.entity_id = ?");
        exp_sql.push_str(" AND je.entity_id = ? AND a.entity_id = ?");
    }

    let mut q1 = sqlx::query(&inc_sql).bind(period);
    let mut q2 = sqlx::query(&exp_sql).bind(period);
    if let Some(e) = eid {
        q1 = q1.bind(e).bind(e);
        q2 = q2.bind(e).bind(e);
    }

    let total_income: i64 = q1
        .fetch_one(&state.db)
        .await
        .map(|r| r.try_get::<i64, _>(0).unwrap_or(0))
        .unwrap_or(0);
    let total_expense: i64 = q2
        .fetch_one(&state.db)
        .await
        .map(|r| r.try_get::<i64, _>(0).unwrap_or(0))
        .unwrap_or(0);

    let net_profit = total_income - total_expense;
    if net_profit == 0 {
        session.push_flash(
            "info",
            format!("Tidak ada laba/rugi untuk periode {}.", period),
        );
        return;
    }

    let accounts = fetch_accounts(state, eid, false).await;
    let retained = accounts
        .iter()
        .find(|a| a.account_name.to_lowercase().contains("retained earnings"))
        .or_else(|| accounts.iter().find(|a| a.account_type == "CAPITAL"));

    let Some(ret_acc) = retained else {
        session.push_flash("warning", "⚠ Akun Retained Earnings tidak ditemukan.");
        return;
    };

    let today = Utc::now().date_naive();
    let desc = format!("Automatic Retained Earnings Adjustment for {}", period);
    if let Ok(ok) = sqlx::query(
        "INSERT INTO ledger_journalentry (entity_id, date, description, created_at, period, is_posted)
         VALUES (?, ?, ?, NOW(6), ?, 1)",
    )
    .bind(eid)
    .bind(today)
    .bind(&desc)
    .bind(period)
    .execute(&state.db)
    .await
    {
        let je_id = ok.last_insert_id() as i64;
        if net_profit > 0 {
            let _ = sqlx::query(
                "INSERT INTO ledger_journalitem (journal_entry_id, account_id, debit, credit, note)
                 VALUES (?, ?, 0, ?, 'Transfer Profit to Retained Earnings')",
            )
            .bind(je_id)
            .bind(ret_acc.id)
            .bind(net_profit)
            .execute(&state.db)
            .await;
        } else {
            let _ = sqlx::query(
                "INSERT INTO ledger_journalitem (journal_entry_id, account_id, debit, credit, note)
                 VALUES (?, ?, ?, 0, 'Transfer Loss to Retained Earnings')",
            )
            .bind(je_id)
            .bind(ret_acc.id)
            .bind(net_profit.abs())
            .execute(&state.db)
            .await;
        }
        session.push_flash(
            "success",
            format!("✅ Retained earnings disesuaikan otomatis untuk periode {}.", period),
        );
    }
}

pub async fn ledger_report(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ModeFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mode = if q.mode == "year" { "year" } else { "period" };

    get_or_create_open_period(&state, eid.as_deref()).await;
    let periods = fetch_closing_periods(&state, eid.as_deref(), false).await;

    let mut selected_period = q.period.trim().to_string();
    if mode == "period" && selected_period.is_empty() {
        if let Some(open) = periods.iter().find(|p| !p.is_closed) {
            selected_period = open.period.clone();
        } else if let Some(first) = periods.first() {
            selected_period = first.period.clone();
        } else {
            selected_period = Utc::now().format("%Y-%m").to_string();
        }
    }

    let mut selected_year = q.year.trim().to_string();
    if mode == "year" && selected_year.is_empty() {
        selected_year = Utc::now().year().to_string();
    }

    let is_period_closed = periods
        .iter()
        .find(|p| p.period == selected_period)
        .map(|p| p.is_closed)
        .unwrap_or(false);

    let accounts = fetch_accounts(&state, eid.as_deref(), false).await;
    let mut ledger_data = Vec::new();

    for acc in accounts {
        let year_num: i32 = selected_year.parse().unwrap_or_else(|_| Utc::now().year());

        // Opening balance
        let mut ob_sql = String::from(
            "SELECT COALESCE(SUM(ji.debit - ji.credit), 0)
             FROM ledger_journalitem ji
             JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
             WHERE ji.account_id = ? AND je.is_posted = 1",
        );
        if mode == "year" {
            ob_sql.push_str(" AND YEAR(je.date) < ?");
        } else {
            ob_sql.push_str(" AND je.period < ?");
        }
        if eid.is_some() {
            ob_sql.push_str(" AND je.entity_id = ?");
        }
        let mut ob_q = sqlx::query(&ob_sql).bind(acc.id);
        if mode == "year" {
            ob_q = ob_q.bind(year_num);
        } else {
            ob_q = ob_q.bind(&selected_period);
        }
        if let Some(ref e) = eid {
            ob_q = ob_q.bind(e);
        }
        let opening_balance: i64 = ob_q
            .fetch_one(&state.db)
            .await
            .map(|r| r.try_get::<i64, _>(0).unwrap_or(0))
            .unwrap_or(0);

        // Transactions in period/year
        let mut it_sql = String::from(
            "SELECT je.date, je.description, ji.debit, ji.credit
             FROM ledger_journalitem ji
             JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
             WHERE ji.account_id = ? AND je.is_posted = 1",
        );
        if mode == "year" {
            it_sql.push_str(" AND YEAR(je.date) = ?");
        } else {
            it_sql.push_str(" AND je.period = ?");
        }
        if eid.is_some() {
            it_sql.push_str(" AND je.entity_id = ?");
        }
        it_sql.push_str(" ORDER BY je.date ASC, ji.id ASC");

        let mut it_q = sqlx::query(&it_sql).bind(acc.id);
        if mode == "year" {
            it_q = it_q.bind(year_num);
        } else {
            it_q = it_q.bind(&selected_period);
        }
        if let Some(ref e) = eid {
            it_q = it_q.bind(e);
        }

        let item_rows = it_q.fetch_all(&state.db).await.unwrap_or_default();
        let mut balance = opening_balance;
        let mut rows = Vec::new();
        for r in item_rows {
            let d: i32 = r.get("debit");
            let c: i32 = r.get("credit");
            let dt: NaiveDate = r.get("date");
            let desc: String = r.get("description");
            balance += (d as i64) - (c as i64);
            rows.push(serde_json::json!({
                "date": dt.format("%Y-%m-%d").to_string(),
                "desc": desc,
                "debit": d,
                "credit": c,
                "balance": balance,
            }));
        }

        ledger_data.push(serde_json::json!({
            "account": acc,
            "rows": rows,
            "opening_balance": opening_balance,
            "closing_balance": balance,
        }));
    }

    let mut ctx = Context::new();
    ctx.insert("ledger_data", &ledger_data);
    ctx.insert("mode", mode);
    ctx.insert("selected_period", &selected_period);
    ctx.insert("selected_year", &selected_year);
    ctx.insert("periods", &periods);
    ctx.insert("is_period_closed", &is_period_closed);
    render_page(&state, &session, "ledger/ledger_report.html", ctx)
}

pub async fn profit_and_loss_report(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ModeFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mode = if q.mode == "year" { "year" } else { "period" };
    let closing_periods = fetch_closing_periods(&state, eid.as_deref(), true).await;

    let mut selected_period = q.period.trim().to_string();
    if mode == "period" {
        if selected_period.is_empty() {
            if let Some(first) = closing_periods.first() {
                selected_period = first.period.clone();
            }
        }
        if !closing_periods.iter().any(|p| p.period == selected_period) {
            selected_period.clear();
        }
    }
    let selected_year = q.year.trim().to_string();

    let accounts = fetch_accounts(&state, eid.as_deref(), true).await;
    let mut income_data = Vec::new();
    let mut expense_data = Vec::new();
    let mut total_income: i64 = 0;
    let mut total_expense: i64 = 0;

    for acc in accounts {
        if acc.account_type != "INCOME" && acc.account_type != "EXPENSES" {
            continue;
        }
        let items =
            fetch_posted_items_for_account(&state, acc.id, eid.as_deref(), mode, &selected_period, &selected_year)
                .await;

        let mut balance: i64 = 0;
        let mut rows = Vec::new();
        let mut subtotal: i64 = 0;

        for (dt, desc, d, c) in items {
            if acc.account_type == "INCOME" {
                balance += c as i64;
                subtotal += c as i64;
            } else {
                balance += (d as i64) - (c as i64);
                subtotal += (d as i64) - (c as i64);
            }
            rows.push(serde_json::json!({
                "date": dt,
                "desc": desc,
                "debit": d,
                "credit": c,
                "balance": balance,
            }));
        }

        if acc.account_type == "INCOME" {
            total_income += subtotal;
            income_data.push(serde_json::json!({
                "account": acc,
                "rows": rows,
                "subtotal": subtotal,
            }));
        } else {
            total_expense += subtotal;
            expense_data.push(serde_json::json!({
                "account": acc,
                "rows": rows,
                "subtotal": subtotal,
            }));
        }
    }

    let mut ctx = Context::new();
    ctx.insert("mode", mode);
    ctx.insert("selected_period", &selected_period);
    ctx.insert("selected_year", &selected_year);
    ctx.insert("closing_periods", &closing_periods);
    ctx.insert("income_data", &income_data);
    ctx.insert("expense_data", &expense_data);
    ctx.insert("total_income", &total_income);
    ctx.insert("total_expense", &total_expense);
    ctx.insert("net_income", &(total_income - total_expense));
    render_page(&state, &session, "ledger/profit_loss.html", ctx)
}

pub async fn balance_sheet_view(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ModeFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mode = if q.mode == "year" { "year" } else { "period" };
    let periods = fetch_closing_periods(&state, eid.as_deref(), false).await;

    let mut selected_period = q.period.trim().to_string();
    if mode == "period" && selected_period.is_empty() {
        if let Some(first) = periods.first() {
            selected_period = first.period.clone();
        }
    }
    let selected_year = q.year.trim().to_string();

    let accounts = fetch_accounts(&state, eid.as_deref(), true).await;
    let mut assets = Vec::new();
    let mut liabilities = Vec::new();
    let mut equities = Vec::new();
    let mut total_assets: i64 = 0;
    let mut total_liabilities: i64 = 0;
    let mut total_equities: i64 = 0;

    for acc in accounts {
        if !matches!(acc.account_type.as_str(), "ASSET" | "LIABILITY" | "CAPITAL") {
            continue;
        }
        let bal = calculate_account_balance(
            &state,
            &acc,
            eid.as_deref(),
            mode,
            &selected_period,
            &selected_year,
            true,
        )
        .await;

        let row = serde_json::json!({ "account": acc, "balance": bal });
        match acc.account_type.as_str() {
            "ASSET" => {
                total_assets += bal;
                assets.push(row);
            }
            "LIABILITY" => {
                total_liabilities += bal;
                liabilities.push(row);
            }
            "CAPITAL" => {
                total_equities += bal;
                equities.push(row);
            }
            _ => {}
        }
    }

    let mut ctx = Context::new();
    ctx.insert("mode", mode);
    ctx.insert("periods", &periods);
    ctx.insert("selected_period", &selected_period);
    ctx.insert("selected_year", &selected_year);
    ctx.insert("assets", &assets);
    ctx.insert("liabilities", &liabilities);
    ctx.insert("equities", &equities);
    ctx.insert("total_assets", &total_assets);
    ctx.insert("total_liabilities", &total_liabilities);
    ctx.insert("total_equities", &total_equities);
    ctx.insert(
        "total_liabilities_equities",
        &(total_liabilities + total_equities),
    );
    render_page(&state, &session, "ledger/balance_sheet.html", ctx)
}

pub async fn profitabilitas_view(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ModeFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mode = if q.mode == "year" { "year" } else { "period" };
    let all_periods = fetch_closing_periods(&state, eid.as_deref(), false).await;

    let mut period = q.period.trim().to_string();
    let year = q.year.trim().to_string();

    if mode == "period" && period.is_empty() {
        if let Some(closed) = all_periods.iter().find(|p| p.is_closed) {
            period = closed.period.clone();
        } else {
            period = get_or_create_open_period(&state, eid.as_deref()).await;
        }
    }

    let accounts = fetch_accounts(&state, eid.as_deref(), true).await;
    let mut total_pendapatan = 0.0_f64;
    let mut total_hpp = 0.0_f64;
    let mut total_biaya = 0.0_f64;
    let mut total_aset = 0.0_f64;
    let mut total_modal = 0.0_f64;

    for acc in accounts {
        let (d, c) =
            sum_account_dc(&state, acc.id, eid.as_deref(), mode, &period, &year, false).await;
        let saldo = if matches!(acc.account_type.as_str(), "ASSET" | "EXPENSES" | "COGS") {
            (d - c) as f64
        } else {
            (c - d) as f64
        };
        match acc.account_type.as_str() {
            "INCOME" => total_pendapatan += saldo,
            "COGS" => total_hpp += saldo,
            "EXPENSES" => total_biaya += saldo,
            "ASSET" => total_aset += saldo,
            "CAPITAL" => total_modal += saldo,
            _ => {}
        }
    }

    let laba_kotor = total_pendapatan - total_hpp;
    let laba_bersih = laba_kotor - total_biaya;

    let ratios = vec![
        ratio_entry("Return on Assets (ROA)", "Laba Bersih / Total Aset", laba_bersih, total_aset),
        ratio_entry("Return on Equity (ROE)", "Laba Bersih / Total Modal", laba_bersih, total_modal),
        ratio_entry("Net Profit Margin (NPM)", "Laba Bersih / Pendapatan", laba_bersih, total_pendapatan),
        ratio_entry("Gross Profit Margin (GPM)", "Laba Kotor / Pendapatan", laba_kotor, total_pendapatan),
    ];

    let mut ctx = Context::new();
    ctx.insert("mode", mode);
    ctx.insert("periode", &period);
    ctx.insert("tahun", &year);
    ctx.insert("periods", &all_periods);
    ctx.insert("pendapatan", &total_pendapatan);
    ctx.insert("hpp", &total_hpp);
    ctx.insert("biaya", &total_biaya);
    ctx.insert("laba_kotor", &laba_kotor);
    ctx.insert("laba_bersih", &laba_bersih);
    ctx.insert("ratios", &ratios);
    render_page(&state, &session, "ledger/profitabilitas.html", ctx)
}

fn ratio_entry(nama: &str, rumus: &str, num: f64, den: f64) -> serde_json::Value {
    let hasil = if den != 0.0 { num / den } else { 0.0 };
    serde_json::json!({
        "nama": nama,
        "rumus": rumus,
        "hasil": format!("{:.2}", hasil),
        "detail": format!("➡️ {:.2} / {:.2} = {:.2}", num, den, hasil),
    })
}

pub async fn solvabilitas_view(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ModeFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let closed_periods = fetch_closing_periods(&state, eid.as_deref(), true).await;
    let mode = if q.mode == "year" { "year" } else { "period" };

    let mut selected_period = q.period.trim().to_string();
    let mut selected_year = q.year.trim().to_string();
    if mode == "year" {
        if selected_year.is_empty() {
            if let Some(first) = closed_periods.first() {
                selected_year = first.period.chars().take(4).collect();
            }
        }
    } else if selected_period.is_empty() {
        if let Some(first) = closed_periods.first() {
            selected_period = first.period.clone();
        }
    }

    let mut ctx = Context::new();
    if selected_period.is_empty() && selected_year.is_empty() {
        ctx.insert("error", "Belum ada periode yang bisa ditampilkan.");
        ctx.insert("periods", &closed_periods);
        ctx.insert("mode", mode);
        return render_page(&state, &session, "ledger/solvabilitas.html", ctx);
    }

    let accounts = fetch_accounts(&state, eid.as_deref(), false).await;
    let (aset_lancar, aset_lancar_detail) =
        balance_by_prefix(&state, &accounts, &["1"], eid.as_deref(), mode, &selected_period, &selected_year).await;
    let (kewajiban_lancar, kewajiban_detail) =
        balance_by_prefix(&state, &accounts, &["2"], eid.as_deref(), mode, &selected_period, &selected_year).await;
    let (ekuitas, ekuitas_detail) =
        balance_by_prefix(&state, &accounts, &["3"], eid.as_deref(), mode, &selected_period, &selected_year).await;
    let (persediaan, persediaan_detail) =
        balance_by_prefix(&state, &accounts, &["1200"], eid.as_deref(), mode, &selected_period, &selected_year).await;

    let rasio_lancar = if kewajiban_lancar != 0.0 { aset_lancar / kewajiban_lancar } else { 0.0 };
    let rasio_cepat = if kewajiban_lancar != 0.0 {
        (aset_lancar - persediaan) / kewajiban_lancar
    } else {
        0.0
    };
    let rasio_utang_aset = if aset_lancar != 0.0 { kewajiban_lancar / aset_lancar } else { 0.0 };
    let rasio_utang_modal = if ekuitas != 0.0 { kewajiban_lancar / ekuitas } else { 0.0 };
    let rasio_ekuitas_aset = if aset_lancar != 0.0 { ekuitas / aset_lancar } else { 0.0 };

    ctx.insert("mode", mode);
    ctx.insert("periods", &closed_periods);
    ctx.insert("period", &selected_period);
    ctx.insert("year", &selected_year);
    ctx.insert("rasio_lancar", &format!("{:.2}", rasio_lancar));
    ctx.insert("rasio_cepat", &format!("{:.2}", rasio_cepat));
    ctx.insert("rasio_utang_aset", &format!("{:.2}", rasio_utang_aset));
    ctx.insert("rasio_utang_modal", &format!("{:.2}", rasio_utang_modal));
    ctx.insert("rasio_ekuitas_aset", &format!("{:.2}", rasio_ekuitas_aset));
    ctx.insert("aset_lancar", &format!("{:.2}", aset_lancar));
    ctx.insert("kewajiban_lancar", &format!("{:.2}", kewajiban_lancar));
    ctx.insert("persediaan", &format!("{:.2}", persediaan));
    ctx.insert("ekuitas", &format!("{:.2}", ekuitas));
    ctx.insert("aset_lancar_detail", &aset_lancar_detail);
    ctx.insert("kewajiban_detail", &kewajiban_detail);
    ctx.insert("persediaan_detail", &persediaan_detail);
    ctx.insert("ekuitas_detail", &ekuitas_detail);
    render_page(&state, &session, "ledger/solvabilitas.html", ctx)
}

pub async fn dimension_report(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<DimensionReportQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();

    let dim_meta_list = [
        ("cost_center", "Cost Center (Pusat Biaya)", "cost_center_id", "ledger_costcenter"),
        ("profit_center", "Profit Center (Pusat Laba)", "profit_center_id", "ledger_profitcenter"),
        ("project", "Project (Proyek)", "project_id", "ledger_project"),
        ("activity", "Activity (Aktivitas / Kegiatan)", "activity_id", "ledger_activity"),
        ("location", "Location (Lokasi / Cabang)", "location_id", "ledger_location"),
        ("partner", "Partner (Customer / Vendor / Mitra)", "partner_id", "ledger_partner"),
        ("product", "Product (Barang / Jasa)", "product_id", "ledger_product"),
    ];

    let dim_type = dim_meta_list
        .iter()
        .find(|(k, _, _, _)| *k == q.dimension.trim())
        .map(|(k, _, _, _)| *k)
        .unwrap_or("cost_center");

    let (_, dim_label, fk_col, dim_table) = dim_meta_list
        .iter()
        .find(|(k, _, _, _)| *k == dim_type)
        .unwrap();

    let mode = match q.mode.as_str() {
        "year" => "year",
        "range" => "range",
        _ => "period",
    };

    let closing_periods = fetch_closing_periods(&state, eid.as_deref(), false).await;
    let mut selected_period = q.period.trim().to_string();
    if mode == "period" && selected_period.is_empty() {
        if let Some(first) = closing_periods.first() {
            selected_period = first.period.clone();
        }
    }

    let dim_objects = fetch_dim_table(&state, dim_table, eid.as_deref()).await;
    let accounts = fetch_accounts(&state, eid.as_deref(), true).await;

    let mut sql = format!(
        "SELECT ji.id, je.id AS entry_id, je.date, je.description, je.period,
                a.id AS account_id, a.account_name, a.account_type, a.coa,
                ji.debit, ji.credit, ji.note, ji.{} AS active_dim_id
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE je.is_posted = 1",
        fk_col
    );
    if eid.is_some() {
        sql.push_str(" AND je.entity_id = ?");
    }
    if mode == "year" && !q.year.trim().is_empty() {
        if let Ok(y) = q.year.trim().parse::<i32>() {
            sql.push_str(&format!(" AND YEAR(je.date) = {}", y));
        }
    } else if mode == "range" && !q.start_date.trim().is_empty() && !q.end_date.trim().is_empty() {
        sql.push_str(" AND je.date BETWEEN ? AND ?");
    } else if mode == "period" && !selected_period.is_empty() {
        sql.push_str(" AND je.period = ?");
    }

    match q.account_type.as_str() {
        "income" => sql.push_str(" AND a.account_type = 'INCOME'"),
        "expense" => sql.push_str(" AND a.account_type IN ('EXPENSES', 'COGS')"),
        "income_expense" => sql.push_str(" AND a.account_type IN ('INCOME', 'EXPENSES', 'COGS')"),
        _ => {}
    }

    if let Ok(acc_id) = q.account_id.trim().parse::<i64>() {
        sql.push_str(&format!(" AND a.id = {}", acc_id));
    }
    sql.push_str(" ORDER BY je.date ASC, je.id ASC, ji.id ASC");

    let mut query = sqlx::query(&sql);
    if let Some(ref e) = eid {
        query = query.bind(e);
    }
    if mode == "range" && !q.start_date.trim().is_empty() && !q.end_date.trim().is_empty() {
        query = query.bind(q.start_date.trim()).bind(q.end_date.trim());
    } else if mode == "period" && !selected_period.is_empty() {
        query = query.bind(&selected_period);
    }

    let items = query.fetch_all(&state.db).await.unwrap_or_default();

    struct DimAgg {
        tx_count: i64,
        debit: i64,
        credit: i64,
        income: i64,
        expense: i64,
    }
    let mut agg_map: HashMap<Option<i64>, DimAgg> = HashMap::new();
    let mut detail_rows = Vec::new();
    let mut running_balance: i64 = 0;

    let dim_name_map: HashMap<i64, (String, String)> = dim_objects
        .iter()
        .map(|d| (d.id, (d.code.clone(), d.name.clone())))
        .collect();

    for r in &items {
        let active_dim: Option<i64> = r.get("active_dim_id");
        let d: i64 = r.get::<i32, _>("debit") as i64;
        let c: i64 = r.get::<i32, _>("credit") as i64;
        let acc_type: String = r.get("account_type");
        let acc_up = acc_type.to_uppercase();

        let entry = agg_map.entry(active_dim).or_insert(DimAgg {
            tx_count: 0,
            debit: 0,
            credit: 0,
            income: 0,
            expense: 0,
        });
        entry.tx_count += 1;
        entry.debit += d;
        entry.credit += c;
        if acc_up == "INCOME" {
            entry.income += c - d;
        } else if acc_up == "EXPENSES" || acc_up == "COGS" {
            entry.expense += d - c;
        }

        let include_detail = if q.dimension_id == "unassigned" {
            active_dim.is_none()
        } else if !q.dimension_id.trim().is_empty() {
            q.dimension_id.trim().parse::<i64>().ok() == active_dim
        } else {
            true
        };
        if include_detail {
            running_balance += d - c;
            let dt: NaiveDate = r.get("date");
            let dim_str = active_dim
                .and_then(|id| dim_name_map.get(&id))
                .map(|(code, name)| format!("{} - {}", code, name))
                .unwrap_or_else(|| "-".to_string());
            detail_rows.push(serde_json::json!({
                "date": dt.format("%Y-%m-%d").to_string(),
                "entry_id": r.get::<i64, _>("entry_id"),
                "description": r.get::<String, _>("description"),
                "period": r.get::<Option<String>, _>("period").unwrap_or_default(),
                "account_code": r.get::<String, _>("coa"),
                "account_name": r.get::<String, _>("account_name"),
                "account_type": acc_type,
                "dimension_value": dim_str,
                "debit": d,
                "credit": c,
                "running_balance": running_balance,
                "note": r.get::<Option<String>, _>("note").unwrap_or_default(),
            }));
        }
    }

    let mut summary_rows = Vec::new();
    let mut grand_debit = 0i64;
    let mut grand_credit = 0i64;
    let mut grand_income = 0i64;
    let mut grand_expense = 0i64;
    let mut grand_tx = 0i64;

    for dobj in &dim_objects {
        if !q.dimension_id.trim().is_empty() && q.dimension_id.trim() != dobj.id.to_string() {
            continue;
        }
        if let Some(agg) = agg_map.get(&Some(dobj.id)) {
            grand_debit += agg.debit;
            grand_credit += agg.credit;
            grand_income += agg.income;
            grand_expense += agg.expense;
            grand_tx += agg.tx_count;
            summary_rows.push(serde_json::json!({
                "id": dobj.id.to_string(),
                "code": dobj.code,
                "name": dobj.name,
                "tx_count": agg.tx_count,
                "debit": agg.debit,
                "credit": agg.credit,
                "net_mutation": agg.debit - agg.credit,
                "income": agg.income,
                "expense": agg.expense,
                "margin": agg.income - agg.expense,
            }));
        }
    }

    if q.dimension_id.trim().is_empty() || q.dimension_id == "unassigned" {
        if let Some(agg) = agg_map.get(&None) {
            grand_debit += agg.debit;
            grand_credit += agg.credit;
            grand_income += agg.income;
            grand_expense += agg.expense;
            grand_tx += agg.tx_count;
            summary_rows.push(serde_json::json!({
                "id": "unassigned",
                "code": "-",
                "name": format!("⚠️ [Tanpa {}]", dim_label.split('(').next().unwrap_or("").trim()),
                "tx_count": agg.tx_count,
                "debit": agg.debit,
                "credit": agg.credit,
                "net_mutation": agg.debit - agg.credit,
                "income": agg.income,
                "expense": agg.expense,
                "margin": agg.income - agg.expense,
            }));
        }
    }

    let dim_choices: Vec<serde_json::Value> = dim_meta_list
        .iter()
        .map(|(k, lbl, _, _)| {
            serde_json::json!({
                "key": k,
                "label": lbl,
                "is_selected": (*k == dim_type),
            })
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("dim_choices", &dim_choices);
    ctx.insert("dim_type", dim_type);
    ctx.insert("dim_label", dim_label);
    ctx.insert("dim_objects", &dim_objects);
    ctx.insert("selected_dim_id", &q.dimension_id);
    ctx.insert("accounts", &accounts);
    ctx.insert("selected_account_id", &q.account_id);
    ctx.insert(
        "account_filter_type",
        if q.account_type.is_empty() {
            "all"
        } else {
            &q.account_type
        },
    );
    ctx.insert("closing_periods", &closing_periods);
    ctx.insert("mode", mode);
    ctx.insert("selected_period", &selected_period);
    ctx.insert("selected_year", &q.year);
    ctx.insert("start_date", &q.start_date);
    ctx.insert("end_date", &q.end_date);
    ctx.insert("summary_rows", &summary_rows);
    ctx.insert("detail_rows", &detail_rows);
    ctx.insert("grand_total_debit", &grand_debit);
    ctx.insert("grand_total_credit", &grand_credit);
    ctx.insert("grand_total_income", &grand_income);
    ctx.insert("grand_total_expense", &grand_expense);
    ctx.insert("grand_total_margin", &(grand_income - grand_expense));
    ctx.insert("grand_total_net_mutation", &(grand_debit - grand_credit));
    ctx.insert("grand_total_transactions", &grand_tx);
    render_page(&state, &session, "ledger/dimension_report.html", ctx)
}

pub async fn dimensions_manage_get(
    State(state): State<AppState>,
    session: SessionContext,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let mut ctx = Context::new();
    insert_dimension_lists(&state, eid.as_deref(), &mut ctx).await;
    render_page(&state, &session, "ledger/dimensions.html", ctx)
}

pub async fn dimensions_create_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<CreateDimensionForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let table = match form.dim_type.as_str() {
        "cost_center" => "ledger_costcenter",
        "profit_center" => "ledger_profitcenter",
        "project" => "ledger_project",
        "activity" => "ledger_activity",
        "location" => "ledger_location",
        "partner" => "ledger_partner",
        "product" => "ledger_product",
        _ => {
            session.push_flash("error", "Jenis dimensi tidak valid.");
            return redirect_with_session(&state, &session.payload, "/ledger/dimensions/");
        }
    };

    let sql = match form.dim_type.as_str() {
        "partner" => format!(
            "INSERT INTO {} (entity_id, partner_type, code, name, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, 1, NOW(6), NOW(6))",
            table
        ),
        "product" => format!(
            "INSERT INTO {} (entity_id, product_type, code, name, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, 1, NOW(6), NOW(6))",
            table
        ),
        "location" => format!(
            "INSERT INTO {} (entity_id, code, name, address, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, 1, NOW(6), NOW(6))",
            table
        ),
        _ => format!(
            "INSERT INTO {} (entity_id, code, name, description, is_active, created_at, updated_at) VALUES (?, ?, ?, ?, 1, NOW(6), NOW(6))",
            table
        ),
    };

    let res = if form.dim_type == "partner" || form.dim_type == "product" {
        let etype = if form.extra_type.trim().is_empty() {
            if form.dim_type == "partner" {
                "customer"
            } else {
                "goods"
            }
        } else {
            form.extra_type.trim()
        };
        sqlx::query(&sql)
            .bind(eid.as_deref())
            .bind(etype)
            .bind(form.code.trim())
            .bind(form.name.trim())
            .execute(&state.db)
            .await
    } else {
        sqlx::query(&sql)
            .bind(eid.as_deref())
            .bind(form.code.trim())
            .bind(form.name.trim())
            .bind(form.description.trim())
            .execute(&state.db)
            .await
    };

    if res.is_ok() {
        session.push_flash("success", "Data master dimensi berhasil ditambahkan.");
    } else {
        session.push_flash("error", "Gagal menambahkan data dimensi.");
    }
    redirect_with_session(&state, &session.payload, "/ledger/dimensions/")
}

// --- Helpers ---

async fn fetch_accounts(state: &AppState, eid: Option<&str>, active_only: bool) -> Vec<AccountView> {
    let mut sql = String::from(
        "SELECT id, entity_id, account_type, account_name, coa, balance_type, active, locked, coa_role_default
         FROM ledger_account WHERE 1=1",
    );
    if active_only {
        sql.push_str(" AND active = 1");
    }
    if eid.is_some() {
        sql.push_str(" AND entity_id = ?");
    }
    sql.push_str(" ORDER BY account_name ASC");
    let mut q = sqlx::query(&sql);
    if let Some(e) = eid {
        q = q.bind(e);
    }
    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| AccountView {
            id: r.get("id"),
            entity_id: r.get("entity_id"),
            account_type: r.get("account_type"),
            account_name: r.get("account_name"),
            coa: r.get("coa"),
            balance_type: r.get("balance_type"),
            active: r.get("active"),
            locked: r.get("locked"),
            coa_role_default: r.get::<Option<String>, _>("coa_role_default").unwrap_or_default(),
        })
        .collect()
}

async fn fetch_dim_table(
    state: &AppState,
    table: &str,
    eid: Option<&str>,
) -> Vec<DimensionItemView> {
    let mut sql = format!(
        "SELECT id, code, name FROM {} WHERE is_active = 1",
        table
    );
    if eid.is_some() {
        sql.push_str(" AND entity_id = ?");
    }
    sql.push_str(" ORDER BY code ASC, name ASC");
    let mut q = sqlx::query(&sql);
    if let Some(e) = eid {
        q = q.bind(e);
    }
    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| DimensionItemView {
            id: r.get("id"),
            code: r.get("code"),
            name: r.get("name"),
            extra: String::new(),
        })
        .collect()
}

async fn insert_dimension_lists(state: &AppState, eid: Option<&str>, ctx: &mut Context) {
    ctx.insert("cost_centers", &fetch_dim_table(state, "ledger_costcenter", eid).await);
    ctx.insert("profit_centers", &fetch_dim_table(state, "ledger_profitcenter", eid).await);
    ctx.insert("projects", &fetch_dim_table(state, "ledger_project", eid).await);
    ctx.insert("activities", &fetch_dim_table(state, "ledger_activity", eid).await);
    ctx.insert("locations", &fetch_dim_table(state, "ledger_location", eid).await);
    ctx.insert("partners", &fetch_dim_table(state, "ledger_partner", eid).await);
    ctx.insert("products", &fetch_dim_table(state, "ledger_product", eid).await);
}

async fn fetch_journal_items(state: &AppState, je_id: i64) -> Vec<JournalItemView> {
    sqlx::query(
        "SELECT ji.id, ji.account_id, a.account_name, a.account_type, a.coa,
                ji.cost_center_id, cc.name AS cc_name,
                ji.profit_center_id, pc.name AS pc_name,
                ji.project_id, prj.name AS prj_name,
                ji.activity_id, act.name AS act_name,
                ji.location_id, loc.name AS loc_name,
                ji.partner_id, prt.name AS prt_name,
                ji.product_id, prd.name AS prd_name,
                ji.debit, ji.credit, ji.note
         FROM ledger_journalitem ji
         JOIN ledger_account a ON a.id = ji.account_id
         LEFT JOIN ledger_costcenter cc ON cc.id = ji.cost_center_id
         LEFT JOIN ledger_profitcenter pc ON pc.id = ji.profit_center_id
         LEFT JOIN ledger_project prj ON prj.id = ji.project_id
         LEFT JOIN ledger_activity act ON act.id = ji.activity_id
         LEFT JOIN ledger_location loc ON loc.id = ji.location_id
         LEFT JOIN ledger_partner prt ON prt.id = ji.partner_id
         LEFT JOIN ledger_product prd ON prd.id = ji.product_id
         WHERE ji.journal_entry_id = ?
         ORDER BY ji.id ASC",
    )
    .bind(je_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| JournalItemView {
        id: r.get("id"),
        account_id: r.get("account_id"),
        account_name: r.get("account_name"),
        account_type: r.get("account_type"),
        coa: r.get("coa"),
        cost_center_id: r.get("cost_center_id"),
        cost_center_name: r.get::<Option<String>, _>("cc_name").unwrap_or_default(),
        profit_center_id: r.get("profit_center_id"),
        profit_center_name: r.get::<Option<String>, _>("pc_name").unwrap_or_default(),
        project_id: r.get("project_id"),
        project_name: r.get::<Option<String>, _>("prj_name").unwrap_or_default(),
        activity_id: r.get("activity_id"),
        activity_name: r.get::<Option<String>, _>("act_name").unwrap_or_default(),
        location_id: r.get("location_id"),
        location_name: r.get::<Option<String>, _>("loc_name").unwrap_or_default(),
        partner_id: r.get("partner_id"),
        partner_name: r.get::<Option<String>, _>("prt_name").unwrap_or_default(),
        product_id: r.get("product_id"),
        product_name: r.get::<Option<String>, _>("prd_name").unwrap_or_default(),
        debit: r.get::<i32, _>("debit") as i64,
        credit: r.get::<i32, _>("credit") as i64,
        note: r.get::<Option<String>, _>("note").unwrap_or_default(),
    })
    .collect()
}

async fn insert_journal_items(state: &AppState, je_id: i64, form: &HashMap<String, Vec<String>>) {
    let empty: Vec<String> = Vec::new();
    let accounts = form.get("account_id[]").unwrap_or(&empty);
    let debits = form.get("debit[]").unwrap_or(&empty);
    let credits = form.get("credit[]").unwrap_or(&empty);
    let notes = form.get("note[]").unwrap_or(&empty);
    let ccs = form.get("cost_center_id[]").unwrap_or(&empty);
    let pcs = form.get("profit_center_id[]").unwrap_or(&empty);
    let prjs = form.get("project_id[]").unwrap_or(&empty);
    let acts = form.get("activity_id[]").unwrap_or(&empty);
    let locs = form.get("location_id[]").unwrap_or(&empty);
    let prts = form.get("partner_id[]").unwrap_or(&empty);
    let prds = form.get("product_id[]").unwrap_or(&empty);

    for (i, acc_raw) in accounts.iter().enumerate() {
        let Ok(account_id) = acc_raw.trim().parse::<i64>() else {
            continue;
        };
        let debit: i32 = debits
            .get(i)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
            .round() as i32;
        let credit: i32 = credits
            .get(i)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(0.0)
            .round() as i32;
        let note = notes.get(i).map(|s| s.trim().to_string()).unwrap_or_default();

        let cc_id = opt_i64(ccs.get(i));
        let pc_id = opt_i64(pcs.get(i));
        let prj_id = opt_i64(prjs.get(i));
        let act_id = opt_i64(acts.get(i));
        let loc_id = opt_i64(locs.get(i));
        let prt_id = opt_i64(prts.get(i));
        let prd_id = opt_i64(prds.get(i));

        let _ = sqlx::query(
            "INSERT INTO ledger_journalitem
             (journal_entry_id, account_id, cost_center_id, profit_center_id, project_id, activity_id, location_id, partner_id, product_id, debit, credit, note)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(je_id)
        .bind(account_id)
        .bind(cc_id)
        .bind(pc_id)
        .bind(prj_id)
        .bind(act_id)
        .bind(loc_id)
        .bind(prt_id)
        .bind(prd_id)
        .bind(debit)
        .bind(credit)
        .bind(note)
        .execute(&state.db)
        .await;
    }
}

async fn fetch_closing_periods(
    state: &AppState,
    eid: Option<&str>,
    closed_only: bool,
) -> Vec<ClosingPeriodView> {
    let mut sql = String::from(
        "SELECT id, period, is_closed, closed_at, closed_by FROM ledger_closingperiod WHERE 1=1",
    );
    if closed_only {
        sql.push_str(" AND is_closed = 1");
    }
    if eid.is_some() {
        sql.push_str(" AND entity_id = ?");
    } else {
        sql.push_str(" AND entity_id IS NULL");
    }
    sql.push_str(" ORDER BY period DESC");

    let mut q = sqlx::query(&sql);
    if let Some(e) = eid {
        q = q.bind(e);
    }
    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let ca: Option<NaiveDateTime> = r.get("closed_at");
            ClosingPeriodView {
                id: r.get("id"),
                period: r.get("period"),
                is_closed: r.get("is_closed"),
                closed_at: ca.map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default(),
                closed_by: r.get::<Option<String>, _>("closed_by").unwrap_or_default(),
            }
        })
        .collect()
}

pub async fn get_or_create_open_period(state: &AppState, eid: Option<&str>) -> String {
    let open: Option<String> = if let Some(e) = eid {
        sqlx::query_scalar(
            "SELECT period FROM ledger_closingperiod WHERE is_closed = 0 AND entity_id = ? ORDER BY period DESC LIMIT 1",
        )
        .bind(e)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_scalar(
            "SELECT period FROM ledger_closingperiod WHERE is_closed = 0 AND entity_id IS NULL ORDER BY period DESC LIMIT 1",
        )
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
    };
    if let Some(p) = open {
        return p;
    }
    let cur = Utc::now().format("%Y-%m").to_string();
    ensure_period_exists(state, &cur, eid).await;
    cur
}

async fn ensure_period_exists(state: &AppState, period: &str, eid: Option<&str>) {
    let exists: Option<i64> = if let Some(e) = eid {
        sqlx::query_scalar(
            "SELECT id FROM ledger_closingperiod WHERE period = ? AND entity_id = ? LIMIT 1",
        )
        .bind(period)
        .bind(e)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
    } else {
        sqlx::query_scalar(
            "SELECT id FROM ledger_closingperiod WHERE period = ? AND entity_id IS NULL LIMIT 1",
        )
        .bind(period)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
    };
    if exists.is_none() {
        let _ = sqlx::query(
            "INSERT IGNORE INTO ledger_closingperiod (entity_id, period, is_closed) VALUES (?, ?, 0)",
        )
        .bind(eid)
        .bind(period)
        .execute(&state.db)
        .await;
    }
}

pub fn next_month_period(period: &str) -> Option<String> {
    let (y_s, m_s) = period.trim().split_once('-')?;
    let y: i32 = y_s.parse().ok()?;
    let m: u32 = m_s.parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    Some(format!("{:04}-{:02}", ny, nm))
}

async fn fetch_posted_items_for_account(
    state: &AppState,
    account_id: i64,
    eid: Option<&str>,
    mode: &str,
    period: &str,
    year: &str,
) -> Vec<(String, String, i32, i32)> {
    let mut sql = String::from(
        "SELECT je.date, je.description, ji.debit, ji.credit
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         WHERE ji.account_id = ? AND je.is_posted = 1",
    );
    if mode == "year" {
        if let Ok(y) = year.parse::<i32>() {
            sql.push_str(&format!(" AND YEAR(je.date) = {}", y));
        } else {
            return Vec::new();
        }
    } else if !period.is_empty() {
        sql.push_str(" AND je.period = ?");
    } else {
        return Vec::new();
    }
    if eid.is_some() {
        sql.push_str(" AND je.entity_id = ?");
    }
    sql.push_str(" ORDER BY je.date ASC, ji.id ASC");

    let mut q = sqlx::query(&sql).bind(account_id);
    if mode != "year" {
        q = q.bind(period);
    }
    if let Some(e) = eid {
        q = q.bind(e);
    }

    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let dt: NaiveDate = r.get("date");
            (
                dt.format("%Y-%m-%d").to_string(),
                r.get("description"),
                r.get("debit"),
                r.get("credit"),
            )
        })
        .collect()
}

async fn sum_account_dc(
    state: &AppState,
    account_id: i64,
    eid: Option<&str>,
    mode: &str,
    period: &str,
    year: &str,
    posted_only: bool,
) -> (i64, i64) {
    let mut sql = String::from(
        "SELECT COALESCE(SUM(ji.debit), 0) AS d, COALESCE(SUM(ji.credit), 0) AS c
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         WHERE ji.account_id = ?",
    );
    if posted_only {
        sql.push_str(" AND je.is_posted = 1");
    }
    if eid.is_some() {
        sql.push_str(" AND je.entity_id = ?");
    }
    if mode == "year" {
        if let Ok(y) = year.parse::<i32>() {
            sql.push_str(&format!(" AND YEAR(je.date) = {}", y));
        }
    } else if !period.is_empty() {
        sql.push_str(" AND je.period = ?");
    }

    let mut q = sqlx::query(&sql).bind(account_id);
    if let Some(e) = eid {
        q = q.bind(e);
    }
    if mode != "year" && !period.is_empty() {
        q = q.bind(period);
    }
    q.fetch_one(&state.db)
        .await
        .map(|r| {
            (
                r.try_get::<i64, _>(0).unwrap_or(0),
                r.try_get::<i64, _>(1).unwrap_or(0),
            )
        })
        .unwrap_or((0, 0))
}

async fn calculate_account_balance(
    state: &AppState,
    acc: &AccountView,
    eid: Option<&str>,
    mode: &str,
    period: &str,
    year: &str,
    posted_only: bool,
) -> i64 {
    let (d, c) = sum_account_dc(state, acc.id, eid, mode, period, year, posted_only).await;
    if acc.balance_type == "Debit" {
        d - c
    } else {
        c - d
    }
}

async fn balance_by_prefix(
    state: &AppState,
    accounts: &[AccountView],
    prefixes: &[&str],
    eid: Option<&str>,
    mode: &str,
    period: &str,
    year: &str,
) -> (f64, Vec<serde_json::Value>) {
    let mut total = 0.0_f64;
    let mut detail = Vec::new();
    for acc in accounts {
        let code = if !acc.coa.is_empty() && acc.coa != "Default CoA" {
            acc.coa.as_str()
        } else {
            acc.account_name.split(':').next().unwrap_or("").trim()
        };
        if !prefixes.iter().any(|p| code.starts_with(p)) {
            continue;
        }
        let bal = calculate_account_balance(state, acc, eid, mode, period, year, false).await as f64;
        total += bal;
        detail.push(serde_json::json!({
            "coa": code,
            "name": acc.account_name,
            "balance": format!("{:.2}", bal),
        }));
    }
    (total, detail)
}

fn parse_multi_form(raw: &[u8]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    let s = String::from_utf8_lossy(raw);
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let k_dec = urlencoding::decode(&k.replace('+', " "))
            .unwrap_or_else(|_| k.into())
            .into_owned();
        let v_dec = urlencoding::decode(&v.replace('+', " "))
            .unwrap_or_else(|_| v.into())
            .into_owned();
        map.entry(k_dec).or_default().push(v_dec);
    }
    map
}

fn first_form_val(map: &HashMap<String, Vec<String>>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.first())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn opt_i64(s: Option<&String>) -> Option<i64> {
    s.and_then(|v| v.trim().parse::<i64>().ok())
}

