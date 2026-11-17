use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path as AxumPath, Query, RawForm, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::{NaiveDate, NaiveDateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tera::Context;

use crate::agent::providers::get_config;
use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::state::AppState;

#[derive(Clone, Debug, Serialize)]
pub struct LookupRow {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChannelView {
    pub id: i64,
    pub akun_id: Option<i64>,
    pub akun_name: String,
    pub for_market_id: Option<i64>,
    pub market_name: String,
    pub channel_id: Option<i64>,
    pub channel_name: String,
    pub funnel_stage_id: Option<i64>,
    pub funnel_stage_name: String,
    pub kategori_biaya_id: Option<i64>,
    pub kategori_biaya_name: String,
    pub amount: String,
    pub rencana_tanggal_posting: String,
    pub tanggal_posting: String,
    pub is_posted: bool,
    pub jenis_konten: String,
    pub judul: String,
    pub isi_konten: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LabeledMetric {
    pub label: String,
    pub value: i64,
    pub raw_key: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PerfView {
    pub id: i64,
    pub period: String,
    pub as_of_date: String,
    pub metrics_map: HashMap<String, i64>,
    pub labeled: Vec<LabeledMetric>,
}

#[derive(Deserialize)]
pub struct ChannelFilterQuery {
    #[serde(default)]
    pub market: String,
    #[serde(default)]
    pub channel: String,
}

#[derive(Deserialize)]
pub struct ChannelFormInput {
    #[serde(default)]
    pub akun: String,
    #[serde(default)]
    pub for_market: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub funnel_stage: String,
    #[serde(default)]
    pub kategori_biaya: String,
    #[serde(default)]
    pub amount: String,
    #[serde(default)]
    pub rencana_tanggal_posting: String,
    #[serde(default)]
    pub tanggal_posting: String,
    #[serde(default)]
    pub is_posted: Option<String>,
    #[serde(default)]
    pub jenis_konten: String,
    pub judul: String,
    #[serde(default)]
    pub isi_konten: String,
}

#[derive(Deserialize)]
pub struct IdeGenerateForm {
    #[serde(default)]
    pub kirim_telegram: Option<String>,
}

pub fn metric_fields_for_platform(platform_raw: &str) -> Vec<&'static str> {
    match platform_raw.to_lowercase().as_str() {
        "ig" | "instagram" => vec![
            "views",
            "view_f",
            "view_nf",
            "account_reached",
            "interaction",
            "profile",
        ],
        "threads" => vec!["impressions", "likes", "replies", "reposts"],
        "linkedin" => vec!["impressions", "reactions", "comments", "shares"],
        "linkedin_newsletter" | "linkedin newsletter" => vec![
            "impressions",
            "member_reached",
            "profile_activity",
            "views",
            "social_engagement",
        ],
        "linkedin_company" | "linkedin company" => vec![
            "impressions",
            "member_reached",
            "engagement",
            "clicks",
            "reactions",
            "comments",
            "repost",
        ],
        "tiktok" => vec!["views", "hearts", "comments", "shares"],
        "blog" | "blog post" => vec!["views", "comments", "clicks"],
        _ => vec![
            "views",
            "impressions",
            "likes",
            "reactions",
            "comments",
            "shares",
        ],
    }
}

pub fn metric_label(platform_raw: &str, key: &str) -> String {
    let p = platform_raw.to_lowercase();
    let lbl = match (p.as_str(), key) {
        ("ig" | "instagram", "view" | "views") => "View",
        ("ig" | "instagram", "view_f") => "View Followers",
        ("ig" | "instagram", "view_nf") => "View Non-Followers",
        ("ig" | "instagram", "account_reached") => "Account Reached",
        ("ig" | "instagram", "interaction") => "Interaction",
        ("ig" | "instagram", "profile") => "Profile Activity",
        ("threads", "impressions") => "Views",
        ("threads", "likes") => "Likes",
        ("threads", "replies") => "Replies",
        ("threads", "reposts") => "Reposts",
        ("linkedin", "impressions") => "Impressions",
        ("linkedin", "reactions") => "Reactions",
        ("linkedin", "comments") => "Comments",
        ("linkedin", "shares") => "Shares",
        ("linkedin_newsletter" | "linkedin newsletter", "impressions") => "Impressions",
        ("linkedin_newsletter" | "linkedin newsletter", "member_reached") => "Members Reached",
        ("linkedin_newsletter" | "linkedin newsletter", "profile_activity") => "Profile Activity",
        ("linkedin_newsletter" | "linkedin newsletter", "views") => "Views",
        ("linkedin_newsletter" | "linkedin newsletter", "social_engagement") => "Social Engagement",
        ("linkedin_company" | "linkedin company", "impressions") => "Impressions",
        ("linkedin_company" | "linkedin company", "member_reached") => "Members Reached",
        ("linkedin_company" | "linkedin company", "engagement") => "Engagement",
        ("linkedin_company" | "linkedin company", "clicks") => "Clicks",
        ("linkedin_company" | "linkedin company", "reactions") => "Reactions",
        ("linkedin_company" | "linkedin company", "comments") => "Comments",
        ("linkedin_company" | "linkedin company", "repost") => "Repost",
        ("tiktok", "views") => "Views",
        ("tiktok", "hearts") => "Hearts",
        ("tiktok", "comments") => "Comments",
        ("tiktok", "shares") => "Shares",
        ("blog" | "blog post", "views") => "Views",
        ("blog" | "blog post", "comments") => "Comments",
        ("blog" | "blog post", "clicks") => "Clicks",
        _ => key,
    };
    lbl.to_string()
}

pub fn format_labeled_metrics(platform: &str, json_str: &str) -> (HashMap<String, i64>, Vec<LabeledMetric>) {
    let mut map: HashMap<String, i64> = HashMap::new();
    if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(json_str) {
        for (k, v) in obj {
            if let Some(num) = v.as_i64() {
                map.insert(k, num);
            }
        }
    }
    let order = [
        "impressions",
        "views",
        "likes",
        "reactions",
        "hearts",
        "comments",
        "shares",
        "clicks",
        "replies",
        "reposts",
    ];
    let mut labeled: Vec<LabeledMetric> = map
        .iter()
        .map(|(k, v)| LabeledMetric {
            label: metric_label(platform, k),
            value: *v,
            raw_key: k.clone(),
        })
        .collect();
    labeled.sort_by_key(|m| {
        order
            .iter()
            .position(|&x| x == m.raw_key)
            .unwrap_or(order.len())
    });
    (map, labeled)
}

pub async fn post_media_home(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    render_page(&state, &session, "post_media/media_home.html", Context::new())
}

pub async fn channel_list(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ChannelFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let markets = fetch_lookup(&state, "post_media_market").await;
    let digital_channels = fetch_lookup(&state, "post_media_digitalchannel").await;
    let channels = fetch_channels(&state, &q.market, &q.channel, false).await;

    let mut market_channels = Vec::new();
    for m in &markets {
        if !q.market.is_empty() && m.name != q.market {
            continue;
        }
        let matching: Vec<&ChannelView> = channels
            .iter()
            .filter(|c| c.for_market_id == Some(m.id))
            .collect();
        if matching.is_empty() {
            continue;
        }
        let mut grouped: BTreeMap<String, Vec<ChannelView>> = BTreeMap::new();
        for ch in matching {
            let cname = if ch.channel_name.is_empty() {
                "Unknown Channel".to_string()
            } else {
                ch.channel_name.clone()
            };
            grouped.entry(cname).or_default().push(ch.clone());
        }
        let groups_vec: Vec<serde_json::Value> = grouped
            .into_iter()
            .map(|(channel_name, mut list)| {
                list.sort_by(|a, b| {
                    a.is_posted.cmp(&b.is_posted).then_with(|| {
                        if !a.is_posted {
                            b.rencana_tanggal_posting.cmp(&a.rencana_tanggal_posting)
                        } else {
                            b.tanggal_posting.cmp(&a.tanggal_posting)
                        }
                    })
                });
                serde_json::json!({ "channel_name": channel_name, "channels": list })
            })
            .collect();

        market_channels.push(serde_json::json!({
            "market": m,
            "groups": groups_vec,
        }));
    }

    let unassigned: Vec<&ChannelView> = channels
        .iter()
        .filter(|c| c.for_market_id.is_none())
        .collect();
    if q.market.is_empty() && !unassigned.is_empty() {
        market_channels.push(serde_json::json!({
            "market": { "id": 0, "name": "Umum / Tanpa Market" },
            "groups": [{ "channel_name": "Konten", "channels": unassigned }],
        }));
    }

    let mut ctx = Context::new();
    ctx.insert("market_channels", &market_channels);
    ctx.insert("markets", &markets);
    ctx.insert("digital_channels", &digital_channels);
    ctx.insert("selected_market", &q.market);
    ctx.insert("selected_channel", &q.channel);
    render_page(&state, &session, "post_media/channel_list.html", ctx)
}

pub async fn channel_detail(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(ch) = fetch_one_channel(&state, pk).await else {
        return Redirect::to("/post_media/").into_response();
    };

    let mut performances = BTreeMap::new();
    for p in ["24h", "1w", "1m"] {
        let perf = get_or_create_perf(&state, pk, p, &ch.channel_name).await;
        performances.insert(p, perf);
    }

    let mut ctx = Context::new();
    ctx.insert("channel", &ch);
    ctx.insert("performances", &performances);
    render_page(&state, &session, "post_media/channel_detail.html", ctx)
}

pub async fn channel_add_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let mut ctx = Context::new();
    ctx.insert("title", "Tambah Channel");
    ctx.insert("is_edit", &false);
    insert_channel_lookups(&state, &mut ctx).await;
    render_page(&state, &session, "post_media/channel_form.html", ctx)
}

pub async fn channel_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<ChannelFormInput>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let _ = sqlx::query(
        "INSERT INTO post_media_channel
         (akun_id, for_market_id, channel_id, funnel_stage_id, kategori_biaya_id, amount,
          rencana_tanggal_posting, tanggal_posting, is_posted, jenis_konten, judul, isi_konten, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NOW(6), NOW(6))",
    )
    .bind(opt_i64_str(&form.akun))
    .bind(opt_i64_str(&form.for_market))
    .bind(opt_i64_str(&form.channel))
    .bind(opt_i64_str(&form.funnel_stage))
    .bind(opt_i64_str(&form.kategori_biaya))
    .bind(form.amount.trim().parse::<Decimal>().ok())
    .bind(NaiveDate::parse_from_str(form.rencana_tanggal_posting.trim(), "%Y-%m-%d").ok())
    .bind(parse_dt_opt(&form.tanggal_posting))
    .bind(form.is_posted.is_some())
    .bind(form.jenis_konten.trim())
    .bind(form.judul.trim())
    .bind(form.isi_konten.trim())
    .execute(&state.db)
    .await;

    session.push_flash("success", "Konten channel berhasil ditambahkan.");
    redirect_with_session(&state, &session.payload, "/post_media/")
}

pub async fn channel_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(ch) = fetch_one_channel(&state, pk).await else {
        return Redirect::to("/post_media/").into_response();
    };
    let mut ctx = Context::new();
    ctx.insert("title", "Edit Channel");
    ctx.insert("is_edit", &true);
    ctx.insert("channel", &ch);
    insert_channel_lookups(&state, &mut ctx).await;
    render_page(&state, &session, "post_media/channel_form.html", ctx)
}

pub async fn channel_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<ChannelFormInput>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let _ = sqlx::query(
        "UPDATE post_media_channel
         SET akun_id = ?, for_market_id = ?, channel_id = ?, funnel_stage_id = ?, kategori_biaya_id = ?,
             amount = ?, rencana_tanggal_posting = ?, tanggal_posting = ?, is_posted = ?,
             jenis_konten = ?, judul = ?, isi_konten = ?, updated_at = NOW(6)
         WHERE id = ?",
    )
    .bind(opt_i64_str(&form.akun))
    .bind(opt_i64_str(&form.for_market))
    .bind(opt_i64_str(&form.channel))
    .bind(opt_i64_str(&form.funnel_stage))
    .bind(opt_i64_str(&form.kategori_biaya))
    .bind(form.amount.trim().parse::<Decimal>().ok())
    .bind(NaiveDate::parse_from_str(form.rencana_tanggal_posting.trim(), "%Y-%m-%d").ok())
    .bind(parse_dt_opt(&form.tanggal_posting))
    .bind(form.is_posted.is_some())
    .bind(form.jenis_konten.trim())
    .bind(form.judul.trim())
    .bind(form.isi_konten.trim())
    .bind(pk)
    .execute(&state.db)
    .await;

    session.push_flash("success", "Konten channel berhasil diperbarui.");
    redirect_with_session(&state, &session.payload, "/post_media/")
}

pub async fn channel_analytics(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ChannelFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let markets = fetch_lookup(&state, "post_media_market").await;
    let digital_channels = fetch_lookup(&state, "post_media_digitalchannel").await;
    let channels = fetch_channels(&state, &q.market, &q.channel, true).await;

    let mut data_by_market = Vec::new();
    for m in &markets {
        let mut list: Vec<ChannelView> = channels
            .iter()
            .filter(|c| c.for_market_id == Some(m.id))
            .cloned()
            .collect();
        if list.is_empty() {
            continue;
        }
        list.sort_by(|a, b| b.tanggal_posting.cmp(&a.tanggal_posting));
        let mut channel_data = Vec::new();
        for ch in list {
            let perf_1m = fetch_latest_1m_perf(&state, ch.id, &ch.channel_name).await;
            channel_data.push(serde_json::json!({
                "obj": ch,
                "perf_1m": perf_1m,
            }));
        }
        data_by_market.push(serde_json::json!({
            "market": m,
            "channels": channel_data,
        }));
    }

    let mut ctx = Context::new();
    ctx.insert("markets", &markets);
    ctx.insert("digital_channels", &digital_channels);
    ctx.insert("selected_market", &q.market);
    ctx.insert("selected_channel_type", &q.channel);
    ctx.insert("data_by_market", &data_by_market);
    render_page(&state, &session, "post_media/channel_analytics.html", ctx)
}

pub async fn export_channel_analytics(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<ChannelFilterQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let channels = fetch_channels(&state, &q.market, &q.channel, true).await;
    let mut csv = String::from("\u{FEFF}Market,Judul,Jenis Konten,Channel,Tanggal Posting,Performance (1 Bulan)\n");

    for ch in channels {
        let perf = fetch_latest_1m_perf(&state, ch.id, &ch.channel_name).await;
        let metrics_str = if let Some(p) = perf {
            if p.labeled.is_empty() {
                "Tidak ada data".to_string()
            } else {
                p.labeled
                    .iter()
                    .map(|m| format!("{}: {}", m.label, m.value))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        } else {
            "Tidak ada data".to_string()
        };
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_escape(&ch.market_name),
            csv_escape(&ch.judul),
            csv_escape(&ch.jenis_konten),
            csv_escape(&ch.channel_name),
            csv_escape(&ch.tanggal_posting),
            csv_escape(&metrics_str),
        ));
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"channel_analytics.csv\""),
    );
    (headers, csv).into_response()
}

pub async fn update_performance_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(ch) = fetch_one_channel(&state, pk).await else {
        return Redirect::to("/post_media/").into_response();
    };
    let mut performances = BTreeMap::new();
    for p in ["24h", "1w", "1m"] {
        performances.insert(p, get_or_create_perf(&state, pk, p, &ch.channel_name).await);
    }
    let metric_fields = metric_fields_for_platform(&ch.channel_name);
    let mut ctx = Context::new();
    ctx.insert("channel", &ch);
    ctx.insert("performances", &performances);
    ctx.insert("metric_fields", &metric_fields);
    render_page(&state, &session, "post_media/update_performance.html", ctx)
}

pub async fn update_performance_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(ch) = fetch_one_channel(&state, pk).await else {
        return Redirect::to("/post_media/").into_response();
    };

    let mut pairs: HashMap<String, String> = HashMap::new();
    for part in String::from_utf8_lossy(&raw).split('&') {
        if let Some((k, v)) = part.split_once('=') {
            let kd = urlencoding::decode(&k.replace('+', " "))
                .unwrap_or_else(|_| k.into())
                .into_owned();
            let vd = urlencoding::decode(&v.replace('+', " "))
                .unwrap_or_else(|_| v.into())
                .into_owned();
            pairs.insert(kd, vd);
        }
    }

    let period = pairs.get("period").map(|s| s.as_str()).unwrap_or("24h");
    if matches!(period, "24h" | "1w" | "1m") {
        let mut perf = get_or_create_perf(&state, pk, period, &ch.channel_name).await;
        let fields = metric_fields_for_platform(&ch.channel_name);
        for f in fields {
            if let Some(raw_v) = pairs.get(f) {
                if let Ok(num) = raw_v.trim().parse::<i64>() {
                    perf.metrics_map.insert(f.to_string(), num);
                }
            }
        }
        let json_str = serde_json::to_string(&perf.metrics_map).unwrap_or_else(|_| "{}".to_string());
        let _ = sqlx::query("UPDATE post_media_channelperformance SET metrics = ? WHERE id = ?")
            .bind(json_str)
            .bind(perf.id)
            .execute(&state.db)
            .await;
    }

    session.push_flash("success", "Performa channel berhasil disimpan.");
    redirect_with_session(
        &state,
        &session.payload,
        &format!("/post_media/channel/{}/", pk),
    )
}

pub async fn ide_konten_list(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let rows = sqlx::query(
        "SELECT id, teks, terkirim_telegram, created_at FROM post_media_idekonten ORDER BY created_at DESC LIMIT 30",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let ide_list: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let dt: NaiveDateTime = r.get("created_at");
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "teks": r.get::<String, _>("teks"),
                "terkirim_telegram": r.get::<bool, _>("terkirim_telegram"),
                "created_at": dt.format("%d %b %Y %H:%M").to_string(),
            })
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("ide_list", &ide_list);
    render_page(&state, &session, "post_media/ide_konten.html", ctx)
}

pub async fn ide_konten_generate(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<IdeGenerateForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let kirim = form.kirim_telegram.as_deref() == Some("on");

    let histori_rows: Vec<String> = sqlx::query_scalar(
        "SELECT teks FROM post_media_idekonten ORDER BY created_at DESC LIMIT 30",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let histori = if histori_rows.is_empty() {
        "Belum ada histori".to_string()
    } else {
        histori_rows.into_iter().rev().collect::<Vec<_>>().join(", ")
    };

    let cfg = get_config(&state.config);
    let api_key = cfg.gemini.api_key;
    if api_key.is_empty() {
        session.push_flash("error", "Gagal generate ide: GEMINI_API_KEY belum diset.");
        return redirect_with_session(&state, &session.payload, "/post_media/ide-konten/");
    }

    let prompt = format!(
        "Buat 1 saja ide konten Instagram untuk edukasi petani sawit swadaya.\n\n\
         Aturan mutlak:\n\
         1. JANGAN gunakan ide yang mirip dengan daftar ini: {}\n\
         2. Hasilkan 1 kalimat pendek yang catchy (maks 10 kata) agar pas dijadikan gambar tipografi.\n\
         3. Langsung berikan teksnya, tanpa tanda kutip, tanpa penjelasan.",
        histori
    );

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        cfg.gemini.model
    );
    let resp = state
        .http_client
        .post(url)
        .header("x-goog-api-key", &api_key)
        .json(&serde_json::json!({
            "contents": [{"parts": [{"text": prompt}]}],
            "generationConfig": {"temperature": 1.2, "topP": 0.95}
        }))
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await;

    let ide_teks = match resp {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or_default();
            body["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string()
        }
        Ok(r) => {
            session.push_flash("error", format!("Gagal generate ide: HTTP {}", r.status()));
            return redirect_with_session(&state, &session.payload, "/post_media/ide-konten/");
        }
        Err(e) => {
            session.push_flash("error", format!("Gagal generate ide: {}", e));
            return redirect_with_session(&state, &session.payload, "/post_media/ide-konten/");
        }
    };

    if ide_teks.is_empty() {
        session.push_flash("error", "Gagal generate ide: respon Gemini kosong.");
        return redirect_with_session(&state, &session.payload, "/post_media/ide-konten/");
    }

    let truncated: String = ide_teks.chars().take(255).collect();
    let inserted = sqlx::query(
        "INSERT INTO post_media_idekonten (teks, terkirim_telegram, created_at) VALUES (?, 0, NOW(6))",
    )
    .bind(&truncated)
    .execute(&state.db)
    .await;

    // Keep max 30 entries
    let keep_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM post_media_idekonten ORDER BY created_at DESC LIMIT 30")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
    if keep_ids.len() == 30 {
        let min_keep = *keep_ids.iter().min().unwrap_or(&0);
        let _ = sqlx::query("DELETE FROM post_media_idekonten WHERE id < ?")
            .bind(min_keep)
            .execute(&state.db)
            .await;
    }

    if kirim {
        let token = &state.config.telegram_bot_token;
        let chat_id = &state.config.telegram_chat_id;
        if token.is_empty() || chat_id.is_empty() {
            session.push_flash(
                "error",
                "Ide dibuat, namun TELEGRAM_BOT_TOKEN / TELEGRAM_CHAT_ID belum diset.",
            );
            return redirect_with_session(&state, &session.payload, "/post_media/ide-konten/");
        }
        let tg_url = format!("https://api.telegram.org/bot{}/sendMessage", token);
        let msg = format!("Ide Instagram Nusasawit hari ini:\n\n{}", truncated);
        let tg_res = state
            .http_client
            .post(tg_url)
            .json(&serde_json::json!({ "chat_id": chat_id, "text": msg }))
            .send()
            .await;
        if let Ok(r) = tg_res {
            if r.status().is_success() {
                if let Ok(ok) = inserted {
                    let _ = sqlx::query(
                        "UPDATE post_media_idekonten SET terkirim_telegram = 1 WHERE id = ?",
                    )
                    .bind(ok.last_insert_id() as i64)
                    .execute(&state.db)
                    .await;
                }
            }
        }
        session.push_flash(
            "success",
            format!("Ide baru dibuat dan dikirim ke Telegram: {}", truncated),
        );
    } else {
        session.push_flash("success", format!("Ide baru dibuat: {}", truncated));
    }

    redirect_with_session(&state, &session.payload, "/post_media/ide-konten/")
}

// --- Helpers ---

async fn fetch_lookup(state: &AppState, table: &str) -> Vec<LookupRow> {
    let sql = format!("SELECT id, name FROM {} ORDER BY name ASC", table);
    sqlx::query(&sql)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| LookupRow {
            id: r.get("id"),
            name: r.get::<Option<String>, _>("name").unwrap_or_default(),
        })
        .collect()
}

async fn insert_channel_lookups(state: &AppState, ctx: &mut Context) {
    ctx.insert("akuns", &fetch_lookup(state, "post_media_akun").await);
    ctx.insert("markets", &fetch_lookup(state, "post_media_market").await);
    ctx.insert(
        "digital_channels",
        &fetch_lookup(state, "post_media_digitalchannel").await,
    );
    ctx.insert(
        "funnel_stages",
        &fetch_lookup(state, "post_media_funnelstage").await,
    );
    ctx.insert("biayas", &fetch_lookup(state, "post_media_biaya").await);
}

async fn fetch_channels(
    state: &AppState,
    market_filter: &str,
    channel_filter: &str,
    channel_by_id: bool,
) -> Vec<ChannelView> {
    let mut sql = String::from(
        "SELECT c.id, c.akun_id, ak.name AS akun_name, c.for_market_id, mk.name AS market_name,
                c.channel_id, dc.name AS channel_name, c.funnel_stage_id, fs.name AS fs_name,
                c.kategori_biaya_id, bya.name AS biaya_name, c.amount,
                c.rencana_tanggal_posting, c.tanggal_posting, c.is_posted,
                c.jenis_konten, c.judul, c.isi_konten
         FROM post_media_channel c
         LEFT JOIN post_media_akun ak ON ak.id = c.akun_id
         LEFT JOIN post_media_market mk ON mk.id = c.for_market_id
         LEFT JOIN post_media_digitalchannel dc ON dc.id = c.channel_id
         LEFT JOIN post_media_funnelstage fs ON fs.id = c.funnel_stage_id
         LEFT JOIN post_media_biaya bya ON bya.id = c.kategori_biaya_id
         WHERE 1=1",
    );
    if !market_filter.is_empty() {
        sql.push_str(" AND mk.name = ?");
    }
    if !channel_filter.is_empty() {
        if channel_by_id && channel_filter.parse::<i64>().is_ok() {
            sql.push_str(" AND c.channel_id = ?");
        } else {
            sql.push_str(" AND dc.name = ?");
        }
    }
    sql.push_str(" ORDER BY c.created_at DESC");

    let mut q = sqlx::query(&sql);
    if !market_filter.is_empty() {
        q = q.bind(market_filter);
    }
    if !channel_filter.is_empty() {
        q = q.bind(channel_filter);
    }
    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(row_to_channel_view)
        .collect()
}

async fn fetch_one_channel(state: &AppState, pk: i64) -> Option<ChannelView> {
    let row = sqlx::query(
        "SELECT c.id, c.akun_id, ak.name AS akun_name, c.for_market_id, mk.name AS market_name,
                c.channel_id, dc.name AS channel_name, c.funnel_stage_id, fs.name AS fs_name,
                c.kategori_biaya_id, bya.name AS biaya_name, c.amount,
                c.rencana_tanggal_posting, c.tanggal_posting, c.is_posted,
                c.jenis_konten, c.judul, c.isi_konten
         FROM post_media_channel c
         LEFT JOIN post_media_akun ak ON ak.id = c.akun_id
         LEFT JOIN post_media_market mk ON mk.id = c.for_market_id
         LEFT JOIN post_media_digitalchannel dc ON dc.id = c.channel_id
         LEFT JOIN post_media_funnelstage fs ON fs.id = c.funnel_stage_id
         LEFT JOIN post_media_biaya bya ON bya.id = c.kategori_biaya_id
         WHERE c.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)?;
    Some(row_to_channel_view(row))
}

fn row_to_channel_view(r: sqlx::mysql::MySqlRow) -> ChannelView {
    let amt: Option<Decimal> = r.get("amount");
    let rtp: Option<NaiveDate> = r.get("rencana_tanggal_posting");
    let tp: Option<NaiveDateTime> = r.get("tanggal_posting");
    ChannelView {
        id: r.get("id"),
        akun_id: r.get("akun_id"),
        akun_name: r.get::<Option<String>, _>("akun_name").unwrap_or_default(),
        for_market_id: r.get("for_market_id"),
        market_name: r.get::<Option<String>, _>("market_name").unwrap_or_default(),
        channel_id: r.get("channel_id"),
        channel_name: r.get::<Option<String>, _>("channel_name").unwrap_or_default(),
        funnel_stage_id: r.get("funnel_stage_id"),
        funnel_stage_name: r.get::<Option<String>, _>("fs_name").unwrap_or_default(),
        kategori_biaya_id: r.get("kategori_biaya_id"),
        kategori_biaya_name: r.get::<Option<String>, _>("biaya_name").unwrap_or_default(),
        amount: amt.map(|d| d.round_dp(2).to_string()).unwrap_or_default(),
        rencana_tanggal_posting: rtp.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
        tanggal_posting: tp.map(|d| d.format("%Y-%m-%dT%H:%M").to_string()).unwrap_or_default(),
        is_posted: r.get("is_posted"),
        jenis_konten: r.get("jenis_konten"),
        judul: r.get("judul"),
        isi_konten: r.get("isi_konten"),
    }
}

async fn get_or_create_perf(
    state: &AppState,
    channel_id: i64,
    period: &str,
    platform: &str,
) -> PerfView {
    let today = Utc::now().date_naive();
    let existing = sqlx::query(
        "SELECT id, period, as_of_date, metrics FROM post_media_channelperformance
         WHERE channel_id = ? AND period = ? ORDER BY as_of_date DESC, id DESC LIMIT 1",
    )
    .bind(channel_id)
    .bind(period)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some(r) = existing {
        let m_str: String = r.get("metrics");
        let (map, labeled) = format_labeled_metrics(platform, &m_str);
        let dt: NaiveDate = r.get("as_of_date");
        return PerfView {
            id: r.get("id"),
            period: r.get("period"),
            as_of_date: dt.format("%Y-%m-%d").to_string(),
            metrics_map: map,
            labeled,
        };
    }

    let ok = sqlx::query(
        "INSERT INTO post_media_channelperformance (channel_id, period, as_of_date, metrics, created_at)
         VALUES (?, ?, ?, '{}', NOW(6))",
    )
    .bind(channel_id)
    .bind(period)
    .bind(today)
    .execute(&state.db)
    .await;

    let id = ok.map(|x| x.last_insert_id() as i64).unwrap_or(0);
    PerfView {
        id,
        period: period.to_string(),
        as_of_date: today.format("%Y-%m-%d").to_string(),
        metrics_map: HashMap::new(),
        labeled: Vec::new(),
    }
}

async fn fetch_latest_1m_perf(
    state: &AppState,
    channel_id: i64,
    platform: &str,
) -> Option<PerfView> {
    let r = sqlx::query(
        "SELECT id, period, as_of_date, metrics FROM post_media_channelperformance
         WHERE channel_id = ? AND period = '1m' ORDER BY as_of_date DESC, id DESC LIMIT 1",
    )
    .bind(channel_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)?;
    let m_str: String = r.get("metrics");
    let (map, labeled) = format_labeled_metrics(platform, &m_str);
    let dt: NaiveDate = r.get("as_of_date");
    Some(PerfView {
        id: r.get("id"),
        period: r.get("period"),
        as_of_date: dt.format("%Y-%m-%d").to_string(),
        metrics_map: map,
        labeled,
    })
}

fn opt_i64_str(s: &str) -> Option<i64> {
    s.trim().parse::<i64>().ok()
}

fn parse_dt_opt(raw: &str) -> Option<NaiveDateTime> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| NaiveDate::parse_from_str(t, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
        .ok()
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
