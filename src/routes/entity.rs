use std::collections::HashMap;
use std::path::Path;

use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::header::REFERER;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;
use tera::Context;

use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::models::{new_uuid_hex, normalize_uuid, Entity, EntityView};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SelectEntityForm {
    pub entity_id: String,
}

async fn load_all_entities(state: &AppState) -> Vec<Entity> {
    crate::db::pg_query_as::<Entity>(
        "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
         FROM entity_entity ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
}

pub async fn entity_list(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let raw = load_all_entities(&state).await;
    let name_map: HashMap<String, String> =
        raw.iter().map(|e| (e.id.clone(), e.name.clone())).collect();

    let entities_list: Vec<EntityView> = raw
        .iter()
        .map(|e| {
            let parent_name = e.parent_id.as_ref().and_then(|pid| name_map.get(pid).cloned());
            EntityView::from_entity(e, parent_name)
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("entities_list", &entities_list);
    render_page(&state, &session, "entity/entity_list.html", ctx)
}

pub async fn entity_create_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let all = load_all_entities(&state).await;
    let mut ctx = Context::new();
    ctx.insert("all_entities", &all);
    ctx.insert("is_edit", &false);
    render_page(&state, &session, "entity/entity_form.html", ctx)
}

pub async fn entity_create_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    multipart: Multipart,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let parsed = parse_entity_multipart(&state, multipart).await;
    let mut ctx = Context::new();
    let all = load_all_entities(&state).await;
    ctx.insert("all_entities", &all);
    ctx.insert("is_edit", &false);

    if parsed.name.is_empty() || parsed.code.is_empty() {
        ctx.insert("error", "Nama dan Kode Perusahaan wajib diisi.");
        return render_page(&state, &session, "entity/entity_form.html", ctx);
    }

    let id = new_uuid_hex();
    let parent_id = if parsed.parent_id.is_empty() {
        None
    } else {
        Some(normalize_uuid(&parsed.parent_id))
    };

    let res = crate::db::pg_query(
        "INSERT INTO entity_entity (id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(&id)
    .bind(&parent_id)
    .bind(&parsed.name)
    .bind(&parsed.code)
    .bind(&parsed.description)
    .bind(&parsed.email)
    .bind(&parsed.phone)
    .bind(&parsed.logo_path)
    .bind(parsed.is_active)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => {
            session.push_flash("success", "Perusahaan berhasil ditambahkan!");
            redirect_with_session(&state, &session.payload, "/entity/")
        }
        Err(e) => {
            ctx.insert("error", &format!("Gagal menyimpan perusahaan: {}", e));
            render_page(&state, &session, "entity/entity_form.html", ctx)
        }
    }
}

pub async fn entity_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<String>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let pk_norm = normalize_uuid(&pk);
    let obj = crate::db::pg_query_as::<Entity>(
        "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
         FROM entity_entity WHERE id = ? LIMIT 1",
    )
    .bind(&pk_norm)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(ent) = obj else {
        return Redirect::to("/entity/").into_response();
    };
    let all: Vec<Entity> = load_all_entities(&state)
        .await
        .into_iter()
        .filter(|e| e.id != ent.id)
        .collect();

    let mut ctx = Context::new();
    ctx.insert("object", &EntityView::from_entity(&ent, None));
    ctx.insert("raw_entity", &ent);
    ctx.insert("all_entities", &all);
    ctx.insert("is_edit", &true);
    render_page(&state, &session, "entity/entity_form.html", ctx)
}

pub async fn entity_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<String>,
    multipart: Multipart,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let pk_norm = normalize_uuid(&pk);
    let existing = crate::db::pg_query_as::<Entity>(
        "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
         FROM entity_entity WHERE id = ? LIMIT 1",
    )
    .bind(&pk_norm)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(ent) = existing else {
        return Redirect::to("/entity/").into_response();
    };

    let parsed = parse_entity_multipart(&state, multipart).await;
    if parsed.name.is_empty() || parsed.code.is_empty() {
        let mut ctx = Context::new();
        ctx.insert("object", &EntityView::from_entity(&ent, None));
        ctx.insert("raw_entity", &ent);
        ctx.insert("all_entities", &load_all_entities(&state).await);
        ctx.insert("is_edit", &true);
        ctx.insert("error", "Nama dan Kode Perusahaan wajib diisi.");
        return render_page(&state, &session, "entity/entity_form.html", ctx);
    }

    let parent_id = if parsed.parent_id.is_empty() {
        None
    } else {
        let p = normalize_uuid(&parsed.parent_id);
        if p == ent.id {
            None
        } else {
            Some(p)
        }
    };
    let logo_final = parsed.logo_path.or(ent.logo.clone());

    let res = crate::db::pg_query(
        "UPDATE entity_entity
         SET parent_id = ?, name = ?, code = ?, description = ?, email = ?, phone = ?, logo = ?, is_active = ?, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?",
    )
    .bind(&parent_id)
    .bind(&parsed.name)
    .bind(&parsed.code)
    .bind(&parsed.description)
    .bind(&parsed.email)
    .bind(&parsed.phone)
    .bind(&logo_final)
    .bind(parsed.is_active)
    .bind(&ent.id)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => {
            session.push_flash("success", "Perusahaan berhasil diperbarui!");
            redirect_with_session(&state, &session.payload, "/entity/")
        }
        Err(e) => {
            let mut ctx = Context::new();
            ctx.insert("object", &EntityView::from_entity(&ent, None));
            ctx.insert("raw_entity", &ent);
            ctx.insert("all_entities", &load_all_entities(&state).await);
            ctx.insert("is_edit", &true);
            ctx.insert("error", &format!("Gagal memperbarui perusahaan: {}", e));
            render_page(&state, &session, "entity/entity_form.html", ctx)
        }
    }
}

pub async fn entity_delete_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<String>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let pk_norm = normalize_uuid(&pk);
    let existing = crate::db::pg_query_as::<Entity>(
        "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
         FROM entity_entity WHERE id = ? LIMIT 1",
    )
    .bind(&pk_norm)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(ent) = existing else {
        return Redirect::to("/entity/").into_response();
    };
    let mut ctx = Context::new();
    ctx.insert("object", &EntityView::from_entity(&ent, None));
    render_page(&state, &session, "entity/entity_confirm_delete.html", ctx)
}

pub async fn entity_delete_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<String>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let pk_norm = normalize_uuid(&pk);
    let _ = crate::db::pg_query("DELETE FROM entity_entity WHERE id = ?")
        .bind(&pk_norm)
        .execute(&state.db)
        .await;

    if normalize_uuid(&session.current_entity_id) == pk_norm {
        session.payload.current_entity_id = "all".to_string();
    }
    session.push_flash("success", "Perusahaan berhasil dihapus!");
    redirect_with_session(&state, &session.payload, "/entity/")
}

pub async fn select_entity(
    State(state): State<AppState>,
    mut session: SessionContext,
    headers: HeaderMap,
    Form(form): Form<SelectEntityForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = form.entity_id.trim().to_string();
    if eid == "all" {
        session.payload.current_entity_id = "all".to_string();
        session.push_flash("success", "Beralih ke semua perusahaan.");
    } else if !eid.is_empty() {
        let norm = normalize_uuid(&eid);
        let found = crate::db::pg_query_as::<Entity>(
            "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
             FROM entity_entity WHERE id = ? AND is_active = 1 LIMIT 1",
        )
        .bind(&norm)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        if let Some(ent) = found {
            session.payload.current_entity_id = ent.id.clone();
            session.push_flash("success", format!("Beralih ke perusahaan: {}", ent.name));
        } else {
            session.push_flash("error", "Gagal beralih perusahaan.");
        }
    }

    let redirect_target = headers
        .get(REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_safe_referer_path)
        .unwrap_or_else(|| "/dashboard/".to_string());

    redirect_with_session(&state, &session.payload, &redirect_target)
}

fn extract_safe_referer_path(referer: &str) -> Option<String> {
    if referer.starts_with('/') && !referer.starts_with("//") {
        return Some(referer.to_string());
    }
    if let Some(idx) = referer.find("://") {
        let rest = &referer[idx + 3..];
        if let Some(slash) = rest.find('/') {
            return Some(rest[slash..].to_string());
        }
    }
    None
}

struct ParsedEntityForm {
    name: String,
    code: String,
    email: String,
    phone: String,
    parent_id: String,
    description: String,
    is_active: bool,
    logo_path: Option<String>,
}

async fn parse_entity_multipart(state: &AppState, mut multipart: Multipart) -> ParsedEntityForm {
    let mut out = ParsedEntityForm {
        name: String::new(),
        code: String::new(),
        email: String::new(),
        phone: String::new(),
        parent_id: String::new(),
        description: String::new(),
        is_active: false,
        logo_path: None,
    };

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "logo" {
            let file_name = field.file_name().unwrap_or("").to_string();
            if !file_name.is_empty() {
                if let Ok(bytes) = field.bytes().await {
                    if !bytes.is_empty() {
                        let safe_name = sanitize_filename(&file_name);
                        let rel = format!(
                            "entities/{}_{}",
                            chrono::Utc::now().timestamp_millis(),
                            safe_name
                        );
                        let dest = state.config.media_root.join(&rel);
                        if let Some(parent) = dest.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        if tokio::fs::write(&dest, &bytes).await.is_ok() {
                            out.logo_path = Some(rel);
                        }
                    }
                }
            }
        } else if let Ok(val) = field.text().await {
            match name.as_str() {
                "name" => out.name = val.trim().to_string(),
                "code" => out.code = val.trim().to_string(),
                "email" => out.email = val.trim().to_string(),
                "phone" => out.phone = val.trim().to_string(),
                "parent" => out.parent_id = val.trim().to_string(),
                "description" => out.description = val.trim().to_string(),
                "is_active" => {
                    out.is_active = val == "on" || val == "true" || val == "1";
                }
                _ => {}
            }
        }
    }

    out
}

pub fn sanitize_filename(raw: &str) -> String {
    let base = Path::new(raw)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    base.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

