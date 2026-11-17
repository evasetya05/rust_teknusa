use std::collections::HashMap;

use axum::extract::{Path as AxumPath, Query, RawForm, State};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::Form;
use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tera::Context;

use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::models::{normalize_uuid, OptionItem};
use crate::state::AppState;

pub const LEAD_SOURCE_CHOICES: &[(&str, &str)] = &[
    ("website", "Website"),
    ("whatsapp", "WhatsApp"),
    ("instagram", "Instagram"),
    ("linkedin", "LinkedIn"),
    ("referral", "Referral"),
    ("fb_ads", "FB Ads"),
    ("teknusa", "Teknusa"),
    ("elf", "Self"),
    ("threads", "Threads"),
    ("other", "Other"),
];

pub const PRIORITY_CHOICES: &[(&str, &str)] = &[
    ("hot", "Hot"),
    ("warm", "Warm"),
    ("cold", "Cold"),
];

pub const STATUS_CHOICES: &[(&str, &str)] = &[
    ("new", "New"),
    ("contacted", "Contacted"),
    ("qualified", "Qualified"),
    ("proposal_sent", "Proposal Sent"),
    ("negotiation", "Negotiation"),
    ("won", "Won"),
    ("lost", "Lost"),
];

pub fn status_label(key: &str) -> &'static str {
    STATUS_CHOICES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("New")
}

pub fn source_label(key: &str) -> &'static str {
    LEAD_SOURCE_CHOICES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
        .unwrap_or("Tidak diketahui")
}

#[derive(Clone, Debug, Serialize)]
pub struct LeadView {
    pub id: i64,
    pub entity_id: Option<String>,
    pub entity_name: String,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub company: String,
    pub account_medsos: String,
    pub lead_source: String,
    pub lead_source_label: String,
    pub assigned_to_id: Option<i32>,
    pub assigned_to_name: String,
    pub priority: String,
    pub priority_label: String,
    pub estimated_value: String,
    pub estimated_value_num: f64,
    pub next_follow_up: String,
    pub status: String,
    pub status_label: String,
    pub created_at: String,
    pub jumlah_interaksi: i64,
    pub interaksi_terakhir: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InteractionView {
    pub id: i64,
    pub lead_id: i64,
    pub lead_name: String,
    pub note: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct LeadListQuery {
    pub lead_source: Option<String>,
}

#[derive(Deserialize)]
pub struct LeadDetailPost {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub note: String,
}

fn choice_options(items: &[(&str, &str)]) -> Vec<OptionItem> {
    items
        .iter()
        .map(|(k, v)| OptionItem {
            value: k.to_string(),
            label: v.to_string(),
        })
        .collect()
}

fn parse_form_pairs(raw: &[u8]) -> HashMap<String, Vec<String>> {
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

fn first_val(map: &HashMap<String, Vec<String>>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.first())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

async fn load_users_options(state: &AppState) -> Vec<OptionItem> {
    let rows = sqlx::query("SELECT id, username, first_name, last_name FROM auth_user WHERE is_active = 1 ORDER BY username")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    rows.into_iter()
        .map(|r| {
            let id: i32 = r.get("id");
            let uname: String = r.get("username");
            let fnm: String = r.get("first_name");
            let lnm: String = r.get("last_name");
            let full = format!("{} {}", fnm.trim(), lnm.trim()).trim().to_string();
            OptionItem {
                value: id.to_string(),
                label: if full.is_empty() { uname } else { full },
            }
        })
        .collect()
}

async fn query_leads(
    state: &AppState,
    entity_id: Option<&str>,
    lead_source: Option<&str>,
) -> Vec<LeadView> {
    let mut sql = String::from(
        "SELECT l.id, l.entity_id, e.name AS entity_name, l.name, l.email, l.phone, l.company,
                l.account_medsos, l.lead_source, l.assigned_to_id,
                u.username AS assigned_username, u.first_name AS assigned_first, u.last_name AS assigned_last,
                l.priority, l.estimated_value, l.next_follow_up, l.status, l.created_at,
                (SELECT COUNT(*) FROM leads_interaction i WHERE i.lead_id = l.id) AS interaction_count,
                (SELECT MAX(i.created_at) FROM leads_interaction i WHERE i.lead_id = l.id) AS last_interaction
         FROM leads_lead l
         LEFT JOIN entity_entity e ON e.id = l.entity_id
         LEFT JOIN auth_user u ON u.id = l.assigned_to_id
         WHERE 1=1",
    );

    if entity_id.is_some() {
        sql.push_str(" AND l.entity_id = ?");
    }
    if let Some(src) = lead_source {
        if !src.is_empty() {
            sql.push_str(" AND l.lead_source = ?");
        }
    }
    sql.push_str(" ORDER BY l.created_at DESC");

    let mut q = sqlx::query(&sql);
    if let Some(eid) = entity_id {
        q = q.bind(eid);
    }
    if let Some(src) = lead_source {
        if !src.is_empty() {
            q = q.bind(src);
        }
    }

    let rows = q.fetch_all(&state.db).await.unwrap_or_default();
    rows.into_iter().map(row_to_lead_view).collect()
}

fn row_to_lead_view(r: sqlx::mysql::MySqlRow) -> LeadView {
    let id: i64 = r.get("id");
    let entity_id: Option<String> = r.get("entity_id");
    let entity_name: Option<String> = r.get("entity_name");
    let name: String = r.get("name");
    let email: Option<String> = r.get("email");
    let phone: Option<String> = r.get("phone");
    let company: Option<String> = r.get("company");
    let account_medsos: Option<String> = r.get("account_medsos");
    let lead_source_opt: Option<String> = r.get("lead_source");
    let assigned_to_id: Option<i32> = r.get("assigned_to_id");
    let assigned_uname: Option<String> = r.get("assigned_username");
    let assigned_first: Option<String> = r.get("assigned_first");
    let assigned_last: Option<String> = r.get("assigned_last");
    let priority: String = r.get("priority");
    let est: Option<Decimal> = r.get("estimated_value");
    let next_fu: Option<NaiveDate> = r.get("next_follow_up");
    let status: String = r.get("status");
    let created_at: NaiveDateTime = r.get("created_at");
    let interaction_count: i64 = r.try_get("interaction_count").unwrap_or(0);
    let last_interaction: Option<NaiveDateTime> = r.try_get("last_interaction").unwrap_or(None);

    let full_assigned = format!(
        "{} {}",
        assigned_first.as_deref().unwrap_or("").trim(),
        assigned_last.as_deref().unwrap_or("").trim()
    )
    .trim()
    .to_string();
    let assigned_to_name = if !full_assigned.is_empty() {
        full_assigned
    } else {
        assigned_uname.unwrap_or_default()
    };

    let src = lead_source_opt.unwrap_or_default();
    let est_num = est.and_then(|d| d.to_f64()).unwrap_or(0.0);
    let est_str = est.map(|d| d.round_dp(2).to_string()).unwrap_or_default();

    LeadView {
        id,
        entity_id,
        entity_name: entity_name.unwrap_or_else(|| "-".to_string()),
        name,
        email: email.unwrap_or_default(),
        phone: phone.unwrap_or_default(),
        company: company.unwrap_or_default(),
        account_medsos: account_medsos.unwrap_or_default(),
        lead_source_label: if src.is_empty() {
            "-".to_string()
        } else {
            source_label(&src).to_string()
        },
        lead_source: src,
        assigned_to_id,
        assigned_to_name,
        priority_label: PRIORITY_CHOICES
            .iter()
            .find(|(k, _)| *k == priority)
            .map(|(_, v)| (*v).to_string())
            .unwrap_or_else(|| priority.clone()),
        priority,
        estimated_value: est_str,
        estimated_value_num: est_num,
        next_follow_up: next_fu.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
        status_label: status_label(&status).to_string(),
        status,
        created_at: created_at.format("%d %b %Y %H:%M").to_string(),
        jumlah_interaksi: interaction_count,
        interaksi_terakhir: last_interaction
            .map(|dt| dt.format("%d %b %Y %H:%M").to_string())
            .unwrap_or_default(),
    }
}

pub async fn lead_list(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<LeadListQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let eid = session.entity_filter_id();
    let src_filter = q.lead_source.as_deref().filter(|s| !s.is_empty());
    let leads = query_leads(&state, eid.as_deref(), src_filter).await;

    let mut ctx = Context::new();
    ctx.insert("leads", &leads);
    ctx.insert("lead_source_choices", &choice_options(LEAD_SOURCE_CHOICES));
    ctx.insert("selected_lead_source", &q.lead_source.unwrap_or_default());
    render_page(&state, &session, "leads/lead_list.html", ctx)
}

pub async fn lead_detail_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let Some(lead) = fetch_one_lead(&state, pk).await else {
        return Redirect::to("/leads/").into_response();
    };
    let interactions = fetch_lead_interactions(&state, pk).await;

    let mut ctx = Context::new();
    ctx.insert("lead", &lead);
    ctx.insert("interactions", &interactions);
    ctx.insert("last_interaction", &interactions.first());
    ctx.insert("status_choices", &choice_options(STATUS_CHOICES));
    render_page(&state, &session, "leads/lead_detail.html", ctx)
}

pub async fn lead_detail_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<LeadDetailPost>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(lead) = fetch_one_lead(&state, pk).await else {
        return Redirect::to("/leads/").into_response();
    };

    if !form.status.trim().is_empty() && form.status.trim() != lead.status {
        let _ = sqlx::query("UPDATE leads_lead SET status = ? WHERE id = ?")
            .bind(form.status.trim())
            .bind(pk)
            .execute(&state.db)
            .await;
    }
    if !form.note.trim().is_empty() {
        let _ = sqlx::query(
            "INSERT INTO leads_interaction (lead_id, note, created_at) VALUES (?, ?, NOW(6))",
        )
        .bind(pk)
        .bind(form.note.trim())
        .execute(&state.db)
        .await;
    }

    session.push_flash("success", "Perubahan lead berhasil disimpan!");
    redirect_with_session(&state, &session.payload, &format!("/leads/lead/{}/", pk))
}

pub async fn lead_create_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let mut ctx = Context::new();
    ctx.insert("is_edit", &false);
    ctx.insert("default_entity_id", &session.entity_filter_id().unwrap_or_default());
    ctx.insert("lead_source_choices", &choice_options(LEAD_SOURCE_CHOICES));
    ctx.insert("priority_choices", &choice_options(PRIORITY_CHOICES));
    ctx.insert("status_choices", &choice_options(STATUS_CHOICES));
    ctx.insert("users_options", &load_users_options(&state).await);
    render_page(&state, &session, "leads/lead_form.html", ctx)
}

pub async fn lead_create_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let form = parse_form_pairs(&raw);
    let name = first_val(&form, "name");
    if name.is_empty() {
        session.push_flash("error", "Nama lead wajib diisi.");
        return redirect_with_session(&state, &session.payload, "/leads/create/");
    }

    let entity_raw = first_val(&form, "entity");
    let entity_id = if entity_raw.is_empty() {
        None
    } else {
        Some(normalize_uuid(&entity_raw))
    };
    let assigned_to_id: Option<i32> = first_val(&form, "assigned_to").parse().ok();
    let est_val: Option<Decimal> = first_val(&form, "estimated_value").parse().ok();
    let next_fu: Option<NaiveDate> =
        NaiveDate::parse_from_str(&first_val(&form, "next_follow_up"), "%Y-%m-%d").ok();
    let priority = non_empty_or(&first_val(&form, "priority"), "warm");
    let status = non_empty_or(&first_val(&form, "status"), "new");
    let lead_source = opt_str(&first_val(&form, "lead_source"));

    let res = sqlx::query(
        "INSERT INTO leads_lead
         (entity_id, name, email, phone, company, account_medsos, lead_source, assigned_to_id, priority, estimated_value, next_follow_up, status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NOW(6))",
    )
    .bind(entity_id)
    .bind(&name)
    .bind(opt_str(&first_val(&form, "email")))
    .bind(opt_str(&first_val(&form, "phone")))
    .bind(opt_str(&first_val(&form, "company")))
    .bind(opt_str(&first_val(&form, "account_medsos")))
    .bind(lead_source)
    .bind(assigned_to_id)
    .bind(priority)
    .bind(est_val)
    .bind(next_fu)
    .bind(status)
    .execute(&state.db)
    .await;

    match res {
        Ok(ok) => {
            let lead_id = ok.last_insert_id() as i64;
            if let Some(notes) = form.get("interaction_note[]") {
                for n in notes {
                    let trimmed = n.trim();
                    if !trimmed.is_empty() {
                        let _ = sqlx::query(
                            "INSERT INTO leads_interaction (lead_id, note, created_at) VALUES (?, ?, NOW(6))",
                        )
                        .bind(lead_id)
                        .bind(trimmed)
                        .execute(&state.db)
                        .await;
                    }
                }
            }
            session.push_flash("success", "Lead berhasil disimpan!");
            redirect_with_session(&state, &session.payload, &format!("/leads/lead/{}/", lead_id))
        }
        Err(e) => {
            session.push_flash("error", format!("Gagal menyimpan lead baru: {}", e));
            redirect_with_session(&state, &session.payload, "/leads/create/")
        }
    }
}

pub async fn lead_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(lead) = fetch_one_lead(&state, pk).await else {
        return Redirect::to("/leads/").into_response();
    };
    let interactions = fetch_lead_interactions(&state, pk).await;

    let mut ctx = Context::new();
    ctx.insert("is_edit", &true);
    ctx.insert("lead", &lead);
    ctx.insert("interactions", &interactions);
    ctx.insert("lead_source_choices", &choice_options(LEAD_SOURCE_CHOICES));
    ctx.insert("priority_choices", &choice_options(PRIORITY_CHOICES));
    ctx.insert("status_choices", &choice_options(STATUS_CHOICES));
    ctx.insert("users_options", &load_users_options(&state).await);
    render_page(&state, &session, "leads/lead_form.html", ctx)
}

pub async fn lead_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let form = parse_form_pairs(&raw);
    let name = first_val(&form, "name");
    if name.is_empty() {
        session.push_flash("error", "Nama lead wajib diisi.");
        return redirect_with_session(&state, &session.payload, &format!("/leads/lead/{}/edit/", pk));
    }

    let entity_raw = first_val(&form, "entity");
    let entity_id = if entity_raw.is_empty() {
        None
    } else {
        Some(normalize_uuid(&entity_raw))
    };
    let assigned_to_id: Option<i32> = first_val(&form, "assigned_to").parse().ok();
    let est_val: Option<Decimal> = first_val(&form, "estimated_value").parse().ok();
    let next_fu: Option<NaiveDate> =
        NaiveDate::parse_from_str(&first_val(&form, "next_follow_up"), "%Y-%m-%d").ok();
    let priority = non_empty_or(&first_val(&form, "priority"), "warm");
    let status = non_empty_or(&first_val(&form, "status"), "new");
    let lead_source = opt_str(&first_val(&form, "lead_source"));

    let _ = sqlx::query(
        "UPDATE leads_lead
         SET entity_id = ?, name = ?, email = ?, phone = ?, company = ?, account_medsos = ?,
             lead_source = ?, assigned_to_id = ?, priority = ?, estimated_value = ?, next_follow_up = ?, status = ?
         WHERE id = ?",
    )
    .bind(entity_id)
    .bind(&name)
    .bind(opt_str(&first_val(&form, "email")))
    .bind(opt_str(&first_val(&form, "phone")))
    .bind(opt_str(&first_val(&form, "company")))
    .bind(opt_str(&first_val(&form, "account_medsos")))
    .bind(lead_source)
    .bind(assigned_to_id)
    .bind(priority)
    .bind(est_val)
    .bind(next_fu)
    .bind(status)
    .bind(pk)
    .execute(&state.db)
    .await;

    if let Some(notes) = form.get("interaction_note[]") {
        for n in notes {
            let trimmed = n.trim();
            if !trimmed.is_empty() {
                let _ = sqlx::query(
                    "INSERT INTO leads_interaction (lead_id, note, created_at) VALUES (?, ?, NOW(6))",
                )
                .bind(pk)
                .bind(trimmed)
                .execute(&state.db)
                .await;
            }
        }
    }

    session.push_flash("success", "Lead berhasil disimpan!");
    redirect_with_session(&state, &session.payload, &format!("/leads/lead/{}/", pk))
}

pub async fn lead_analysis(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let eid = session.entity_filter_id();
    let leads = query_leads(&state, eid.as_deref(), None).await;
    let stats = compute_lead_stats(&leads);

    let mut int_sql = String::from(
        "SELECT i.id, i.lead_id, l.name AS lead_name, i.note, i.created_at
         FROM leads_interaction i
         JOIN leads_lead l ON l.id = i.lead_id
         WHERE 1=1",
    );
    if eid.is_some() {
        int_sql.push_str(" AND l.entity_id = ?");
    }
    int_sql.push_str(" ORDER BY i.created_at DESC LIMIT 8");

    let mut q = sqlx::query(&int_sql);
    if let Some(ref e) = eid {
        q = q.bind(e);
    }
    let recent_interactions: Vec<InteractionView> = q
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let dt: NaiveDateTime = r.get("created_at");
            InteractionView {
                id: r.get("id"),
                lead_id: r.get("lead_id"),
                lead_name: r.get("lead_name"),
                note: r.get("note"),
                created_at: dt.format("%d %b %Y %H:%M").to_string(),
            }
        })
        .collect();

    let recent_leads: Vec<LeadView> = leads.into_iter().take(8).collect();

    let mut ctx = Context::new();
    ctx.insert("metrics", &stats.metrics);
    ctx.insert("breakdown", &stats.breakdown);
    ctx.insert("recent_leads", &recent_leads);
    ctx.insert("recent_interactions", &recent_interactions);
    render_page(&state, &session, "leads/lead_analisis.html", ctx)
}

pub async fn lead_graphic(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let eid = session.entity_filter_id();
    let leads = query_leads(&state, eid.as_deref(), None).await;
    let stats = compute_lead_stats(&leads);

    let today = Utc::now().date_naive();
    let start_date = today - Duration::days(13);

    let mut lead_day_sql = String::from(
        "SELECT DATE(created_at) AS d, COUNT(*) AS cnt FROM leads_lead WHERE DATE(created_at) >= ?",
    );
    if eid.is_some() {
        lead_day_sql.push_str(" AND entity_id = ?");
    }
    lead_day_sql.push_str(" GROUP BY DATE(created_at)");

    let mut q1 = sqlx::query(&lead_day_sql).bind(start_date);
    if let Some(ref e) = eid {
        q1 = q1.bind(e);
    }
    let mut leads_map: HashMap<NaiveDate, i64> = HashMap::new();
    for r in q1.fetch_all(&state.db).await.unwrap_or_default() {
        let d: NaiveDate = r.get("d");
        let c: i64 = r.get("cnt");
        leads_map.insert(d, c);
    }

    let mut int_day_sql = String::from(
        "SELECT DATE(i.created_at) AS d, COUNT(*) AS cnt
         FROM leads_interaction i
         JOIN leads_lead l ON l.id = i.lead_id
         WHERE DATE(i.created_at) >= ?",
    );
    if eid.is_some() {
        int_day_sql.push_str(" AND l.entity_id = ?");
    }
    int_day_sql.push_str(" GROUP BY DATE(i.created_at)");

    let mut q2 = sqlx::query(&int_day_sql).bind(start_date);
    if let Some(ref e) = eid {
        q2 = q2.bind(e);
    }
    let mut int_map: HashMap<NaiveDate, i64> = HashMap::new();
    for r in q2.fetch_all(&state.db).await.unwrap_or_default() {
        let d: NaiveDate = r.get("d");
        let c: i64 = r.get("cnt");
        int_map.insert(d, c);
    }

    let mut labels = Vec::with_capacity(14);
    let mut lead_counts = Vec::with_capacity(14);
    let mut interaction_counts = Vec::with_capacity(14);
    for idx in 0..14 {
        let day = start_date + Duration::days(idx);
        labels.push(day.format("%d %b").to_string());
        lead_counts.push(*leads_map.get(&day).unwrap_or(&0));
        interaction_counts.push(*int_map.get(&day).unwrap_or(&0));
    }

    let mut ctx = Context::new();
    ctx.insert("metrics", &stats.metrics);
    ctx.insert("breakdown", &stats.breakdown);
    ctx.insert(
        "charts",
        &serde_json::json!({
            "labels": labels,
            "lead_counts": lead_counts,
            "interaction_counts": interaction_counts,
        }),
    );
    render_page(&state, &session, "leads/lead_graphic.html", ctx)
}

pub async fn lead_calendar(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    render_page(&state, &session, "leads/lead_calendar.html", Context::new())
}

pub async fn lead_calendar_events(
    State(state): State<AppState>,
    session: SessionContext,
) -> Response {
    if session.user.is_none() {
        return Json(serde_json::json!([])).into_response();
    }
    let eid = session.entity_filter_id();
    let mut sql = String::from(
        "SELECT id, name, next_follow_up, status FROM leads_lead WHERE next_follow_up IS NOT NULL",
    );
    if eid.is_some() {
        sql.push_str(" AND entity_id = ?");
    }
    let mut q = sqlx::query(&sql);
    if let Some(ref e) = eid {
        q = q.bind(e);
    }
    let events: Vec<serde_json::Value> = q
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let name: String = r.get("name");
            let dt: NaiveDate = r.get("next_follow_up");
            let status: String = r.get("status");
            let color = if status == "won" { "#10b981" } else { "#2563eb" };
            serde_json::json!({
                "title": format!("Follow-up: {}", name),
                "start": dt.format("%Y-%m-%d").to_string(),
                "url": format!("/leads/lead/{}/", id),
                "allDay": true,
                "backgroundColor": color,
                "borderColor": color,
                "textColor": "#ffffff",
            })
        })
        .collect();

    Json(events).into_response()
}

async fn fetch_one_lead(state: &AppState, pk: i64) -> Option<LeadView> {
    let row = sqlx::query(
        "SELECT l.id, l.entity_id, e.name AS entity_name, l.name, l.email, l.phone, l.company,
                l.account_medsos, l.lead_source, l.assigned_to_id,
                u.username AS assigned_username, u.first_name AS assigned_first, u.last_name AS assigned_last,
                l.priority, l.estimated_value, l.next_follow_up, l.status, l.created_at,
                (SELECT COUNT(*) FROM leads_interaction i WHERE i.lead_id = l.id) AS interaction_count,
                (SELECT MAX(i.created_at) FROM leads_interaction i WHERE i.lead_id = l.id) AS last_interaction
         FROM leads_lead l
         LEFT JOIN entity_entity e ON e.id = l.entity_id
         LEFT JOIN auth_user u ON u.id = l.assigned_to_id
         WHERE l.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)?;
    Some(row_to_lead_view(row))
}

async fn fetch_lead_interactions(state: &AppState, lead_id: i64) -> Vec<InteractionView> {
    sqlx::query(
        "SELECT i.id, i.lead_id, l.name AS lead_name, i.note, i.created_at
         FROM leads_interaction i
         JOIN leads_lead l ON l.id = i.lead_id
         WHERE i.lead_id = ?
         ORDER BY i.created_at DESC",
    )
    .bind(lead_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        let dt: NaiveDateTime = r.get("created_at");
        InteractionView {
            id: r.get("id"),
            lead_id: r.get("lead_id"),
            lead_name: r.get("lead_name"),
            note: r.get("note"),
            created_at: dt.format("%d %b %Y %H:%M").to_string(),
        }
    })
    .collect()
}

struct LeadStats {
    metrics: serde_json::Value,
    breakdown: serde_json::Value,
}

fn compute_lead_stats(leads: &[LeadView]) -> LeadStats {
    let total_leads = leads.len();
    let active_leads = leads
        .iter()
        .filter(|l| l.status != "won" && l.status != "lost")
        .count();
    let won_leads = leads.iter().filter(|l| l.status == "won").count();
    let conversion_rate = if total_leads > 0 {
        (won_leads as f64 / total_leads as f64) * 100.0
    } else {
        0.0
    };

    let today = Utc::now().date_naive();
    let open_statuses = ["new", "contacted", "qualified", "proposal_sent", "negotiation"];
    let mut pending_followups = 0usize;
    let mut overdue_followups = 0usize;

    let mut status_map: HashMap<String, usize> = HashMap::new();
    let mut source_map: HashMap<String, usize> = HashMap::new();

    for l in leads {
        *status_map.entry(l.status.clone()).or_insert(0) += 1;
        *source_map.entry(l.lead_source.clone()).or_insert(0) += 1;

        if open_statuses.contains(&l.status.as_str()) {
            if let Ok(d) = NaiveDate::parse_from_str(&l.next_follow_up, "%Y-%m-%d") {
                if d >= today {
                    pending_followups += 1;
                } else {
                    overdue_followups += 1;
                }
            }
        }
    }

    let mut breakdown_status: Vec<serde_json::Value> = status_map
        .into_iter()
        .map(|(k, c)| {
            let pct = if total_leads > 0 {
                (c as f64 / total_leads as f64) * 100.0
            } else {
                0.0
            };
            serde_json::json!({
                "key": k,
                "label": status_label(&k),
                "count": c,
                "percentage": format!("{:.1}", pct),
            })
        })
        .collect();
    breakdown_status.sort_by(|a, b| {
        b["count"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["count"].as_u64().unwrap_or(0))
    });

    let mut breakdown_sources: Vec<serde_json::Value> = source_map
        .into_iter()
        .map(|(k, c)| {
            let lbl = if k.is_empty() {
                "Tidak diketahui"
            } else {
                source_label(&k)
            };
            serde_json::json!({
                "key": if k.is_empty() { "-" } else { k.as_str() },
                "label": lbl,
                "count": c,
            })
        })
        .collect();
    breakdown_sources.sort_by(|a, b| {
        b["count"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["count"].as_u64().unwrap_or(0))
    });

    LeadStats {
        metrics: serde_json::json!({
            "total_leads": total_leads,
            "active_leads": active_leads,
            "won_leads": won_leads,
            "conversion_rate": format!("{:.1}", conversion_rate),
            "pending_followups": pending_followups,
            "overdue_followups": overdue_followups,
        }),
        breakdown: serde_json::json!({
            "status": breakdown_status,
            "sources": breakdown_sources,
        }),
    }
}

fn opt_str(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn non_empty_or(s: &str, default: &str) -> String {
    let t = s.trim();
    if t.is_empty() {
        default.to_string()
    } else {
        t.to_string()
    }
}
