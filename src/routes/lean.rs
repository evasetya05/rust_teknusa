use axum::extract::{Path as AxumPath, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tera::Context;

use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::models::OptionItem;
use crate::state::AppState;

const STATUS_CHOICES: &[(&str, &str)] = &[
    ("draft", "Draft"),
    ("in_progress", "In Progress"),
    ("done", "Done"),
    ("archived", "Archived"),
];

fn status_options() -> Vec<OptionItem> {
    STATUS_CHOICES
        .iter()
        .map(|(v, l)| OptionItem {
            value: (*v).to_string(),
            label: (*l).to_string(),
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct IdeaView {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub assumptions: String,
    pub priority: i32,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BuildView {
    pub id: i64,
    pub idea_id: i64,
    pub title: String,
    pub description: String,
    pub link: String,
    pub status: String,
    pub products: Vec<ProductView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProductView {
    pub id: i64,
    pub build_id: i64,
    pub name: String,
    pub description: String,
    pub public: bool,
    pub measures: Vec<MeasureView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeasureView {
    pub id: i64,
    pub product_id: i64,
    pub metric: String,
    pub target: Option<f64>,
    pub current: Option<f64>,
    pub notes: String,
    pub datapoints: Vec<DataPointView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DataPointView {
    pub id: i64,
    pub measure_id: i64,
    pub value: f64,
    pub measured_at: String,
    pub note: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LearningView {
    pub id: i64,
    pub idea_id: Option<i64>,
    pub summary: String,
    pub action: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct IdeaForm {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub assumptions: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub status: String,
}

#[derive(Deserialize)]
pub struct BuildForm {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Deserialize)]
pub struct ProductForm {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub public: Option<String>,
}

#[derive(Deserialize)]
pub struct MeasureForm {
    pub metric: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub current: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Deserialize)]
pub struct DataPointForm {
    pub value: f64,
    #[serde(default)]
    pub measured_at: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Deserialize)]
pub struct LearningForm {
    pub summary: String,
    #[serde(default)]
    pub action: String,
}

pub async fn idea_list(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let rows = crate::db::pg_query(
        "SELECT id, title, description, assumptions, priority, status, created_at
         FROM lean_idea ORDER BY priority DESC, created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let ideas: Vec<IdeaView> = rows
        .into_iter()
        .map(|r| {
            let dt: NaiveDateTime = r.get("created_at");
            IdeaView {
                id: r.get("id"),
                title: r.get("title"),
                description: r.get("description"),
                assumptions: r.get("assumptions"),
                priority: r.get("priority"),
                status: r.get("status"),
                created_at: dt.format("%d %b %Y").to_string(),
            }
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("ideas", &ideas);
    render_page(&state, &session, "lean/idea_list.html", ctx)
}

pub async fn idea_detail(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let row = crate::db::pg_query(
        "SELECT id, title, description, assumptions, priority, status, created_at FROM lean_idea WHERE id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let dt: NaiveDateTime = r.get("created_at");
    let idea = IdeaView {
        id: r.get("id"),
        title: r.get("title"),
        description: r.get("description"),
        assumptions: r.get("assumptions"),
        priority: r.get("priority"),
        status: r.get("status"),
        created_at: dt.format("%d %b %Y").to_string(),
    };

    let build_rows = crate::db::pg_query(
        "SELECT id, idea_id, title, description, link, status FROM lean_build WHERE idea_id = ? ORDER BY id ASC",
    )
    .bind(pk)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut builds = Vec::new();
    let mut flat_products = Vec::new();
    let mut flat_measures = Vec::new();
    let mut flat_datapoints = Vec::new();

    for br in build_rows {
        let bid: i64 = br.get("id");
        let prod_rows = crate::db::pg_query(
            "SELECT id, build_id, name, description, public FROM lean_product WHERE build_id = ? ORDER BY id ASC",
        )
        .bind(bid)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let mut products = Vec::new();
        for pr in prod_rows {
            let pid: i64 = pr.get("id");
            let meas_rows = crate::db::pg_query(
                "SELECT id, product_id, metric, target, current, notes FROM lean_measure WHERE product_id = ? ORDER BY id ASC",
            )
            .bind(pid)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

            let mut measures = Vec::new();
            for mr in meas_rows {
                let mid: i64 = mr.get("id");
                let dp_rows = crate::db::pg_query(
                    "SELECT id, measure_id, value, measured_at, note FROM lean_datapoint WHERE measure_id = ? ORDER BY measured_at DESC",
                )
                .bind(mid)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();

                let mut dps = Vec::new();
                for dr in dp_rows {
                    let mdt: NaiveDateTime = dr.get("measured_at");
                    let dp = DataPointView {
                        id: dr.get("id"),
                        measure_id: dr.get("measure_id"),
                        value: dr.get("value"),
                        measured_at: mdt.format("%Y-%m-%d %H:%M").to_string(),
                        note: dr.get("note"),
                    };
                    flat_datapoints.push(dp.clone());
                    dps.push(dp);
                }

                let m = MeasureView {
                    id: mid,
                    product_id: mr.get("product_id"),
                    metric: mr.get("metric"),
                    target: mr.get("target"),
                    current: mr.get("current"),
                    notes: mr.get("notes"),
                    datapoints: dps,
                };
                flat_measures.push(m.clone());
                measures.push(m);
            }

            let p = ProductView {
                id: pid,
                build_id: pr.get("build_id"),
                name: pr.get("name"),
                description: pr.get("description"),
                public: pr.get("public"),
                measures,
            };
            flat_products.push(p.clone());
            products.push(p);
        }

        builds.push(BuildView {
            id: bid,
            idea_id: br.get("idea_id"),
            title: br.get("title"),
            description: br.get("description"),
            link: br.get("link"),
            status: br.get("status"),
            products,
        });
    }

    let learnings: Vec<LearningView> = crate::db::pg_query(
        "SELECT id, idea_id, summary, action, created_at FROM lean_learning WHERE idea_id = ? ORDER BY created_at DESC",
    )
    .bind(pk)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|lr| {
        let ldt: NaiveDateTime = lr.get("created_at");
        LearningView {
            id: lr.get("id"),
            idea_id: lr.get("idea_id"),
            summary: lr.get("summary"),
            action: lr.get("action"),
            created_at: ldt.format("%d %b %Y").to_string(),
        }
    })
    .collect();

    let mut ctx = Context::new();
    ctx.insert("object", &idea);
    ctx.insert("builds", &builds);
    ctx.insert("products", &flat_products);
    ctx.insert("measures", &flat_measures);
    ctx.insert("datapoints", &flat_datapoints);
    ctx.insert("learnings", &learnings);
    ctx.insert("status_choices", &status_options());
    render_page(&state, &session, "lean/idea_detail.html", ctx)
}

pub async fn idea_create_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let mut ctx = Context::new();
    ctx.insert("title", "Create Idea");
    ctx.insert("item_type", "idea");
    ctx.insert("status_choices", &status_options());
    ctx.insert("back_url", "/lean/");
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn idea_create_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<IdeaForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let status = if form.status.trim().is_empty() {
        "draft"
    } else {
        form.status.trim()
    };
    let res = crate::db::pg_query(
        "INSERT INTO lean_idea (title, description, assumptions, priority, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP) RETURNING id",
    )
    .bind(form.title.trim())
    .bind(form.description.trim())
    .bind(form.assumptions.trim())
    .bind(form.priority)
    .bind(status)
    .fetch_one(&state.db)
    .await;

    match res {
        Ok(ok) => {
            session.push_flash("success", "Idea berhasil dibuat.");
            redirect_with_session(
                &state,
                &session.payload,
                &format!("/lean/idea/{}/", ok.get::<i64, _>("id")),
            )
        }
        Err(e) => {
            session.push_flash("error", format!("Gagal membuat idea: {}", e));
            redirect_with_session(&state, &session.payload, "/lean/")
        }
    }
}

pub async fn idea_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = crate::db::pg_query(
        "SELECT id, title, description, assumptions, priority, status, created_at FROM lean_idea WHERE id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let dt: NaiveDateTime = r.get("created_at");
    let idea = IdeaView {
        id: r.get("id"),
        title: r.get("title"),
        description: r.get("description"),
        assumptions: r.get("assumptions"),
        priority: r.get("priority"),
        status: r.get("status"),
        created_at: dt.format("%d %b %Y").to_string(),
    };
    let mut ctx = Context::new();
    ctx.insert("title", &format!("Edit Idea: {}", idea.title));
    ctx.insert("item_type", "idea");
    ctx.insert("item", &idea);
    ctx.insert("status_choices", &status_options());
    ctx.insert("back_url", &format!("/lean/idea/{}/", pk));
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn idea_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<IdeaForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let status = if form.status.trim().is_empty() {
        "draft"
    } else {
        form.status.trim()
    };
    let _ = crate::db::pg_query(
        "UPDATE lean_idea SET title = ?, description = ?, assumptions = ?, priority = ?, status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(form.title.trim())
    .bind(form.description.trim())
    .bind(form.assumptions.trim())
    .bind(form.priority)
    .bind(status)
    .bind(pk)
    .execute(&state.db)
    .await;

    session.push_flash("success", "Idea diperbarui.");
    redirect_with_session(&state, &session.payload, &format!("/lean/idea/{}/", pk))
}

pub async fn build_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(idea_id): AxumPath<i64>,
    Form(form): Form<BuildForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let status = if form.status.trim().is_empty() {
        "in_progress"
    } else {
        form.status.trim()
    };
    let _ = crate::db::pg_query(
        "INSERT INTO lean_build (idea_id, title, description, link, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(idea_id)
    .bind(form.title.trim())
    .bind(form.description.trim())
    .bind(form.link.trim())
    .bind(status)
    .execute(&state.db)
    .await;

    session.push_flash("success", "Build ditambahkan.");
    redirect_with_session(&state, &session.payload, &format!("/lean/idea/{}/", idea_id))
}

pub async fn build_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = crate::db::pg_query(
        "SELECT id, idea_id, title, description, link, status FROM lean_build WHERE id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let idea_id: i64 = r.get("idea_id");
    let title: String = r.get("title");
    let mut ctx = Context::new();
    ctx.insert("title", &format!("Edit Build: {}", title));
    ctx.insert("item_type", "build");
    ctx.insert(
        "item",
        &serde_json::json!({
            "id": pk,
            "idea_id": idea_id,
            "title": title,
            "description": r.get::<String, _>("description"),
            "link": r.get::<String, _>("link"),
            "status": r.get::<String, _>("status"),
        }),
    );
    ctx.insert("status_choices", &status_options());
    ctx.insert("back_url", &format!("/lean/idea/{}/", idea_id));
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn build_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<BuildForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar("SELECT idea_id FROM lean_build WHERE id = ?")
        .bind(pk)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    let _ = crate::db::pg_query(
        "UPDATE lean_build SET title = ?, description = ?, link = ?, status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(form.title.trim())
    .bind(form.description.trim())
    .bind(form.link.trim())
    .bind(if form.status.trim().is_empty() { "in_progress" } else { form.status.trim() })
    .bind(pk)
    .execute(&state.db)
    .await;
    session.push_flash("success", "Build diperbarui.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn product_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(build_id): AxumPath<i64>,
    Form(form): Form<ProductForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar("SELECT idea_id FROM lean_build WHERE id = ?")
        .bind(build_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    let is_pub = form.public.is_some();
    let _ = crate::db::pg_query(
        "INSERT INTO lean_product (build_id, name, description, public, created_at, updated_at)
         VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(build_id)
    .bind(form.name.trim())
    .bind(form.description.trim())
    .bind(is_pub)
    .execute(&state.db)
    .await;
    session.push_flash("success", "Product ditambahkan.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn product_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = crate::db::pg_query(
        "SELECT p.id, p.build_id, p.name, p.description, p.public, b.idea_id
         FROM lean_product p JOIN lean_build b ON b.id = p.build_id WHERE p.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let idea_id: i64 = r.get("idea_id");
    let name: String = r.get("name");
    let mut ctx = Context::new();
    ctx.insert("title", &format!("Edit Product: {}", name));
    ctx.insert("item_type", "product");
    ctx.insert(
        "item",
        &serde_json::json!({
            "id": pk,
            "name": name,
            "description": r.get::<String, _>("description"),
            "public": r.get::<bool, _>("public"),
        }),
    );
    ctx.insert("back_url", &format!("/lean/idea/{}/", idea_id));
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn product_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<ProductForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar(
        "SELECT b.idea_id FROM lean_product p JOIN lean_build b ON b.id = p.build_id WHERE p.id = ?",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let is_pub = form.public.is_some();
    let _ = crate::db::pg_query(
        "UPDATE lean_product SET name = ?, description = ?, public = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(form.name.trim())
    .bind(form.description.trim())
    .bind(is_pub)
    .bind(pk)
    .execute(&state.db)
    .await;
    session.push_flash("success", "Product diperbarui.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn measure_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(product_id): AxumPath<i64>,
    Form(form): Form<MeasureForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar(
        "SELECT b.idea_id FROM lean_product p JOIN lean_build b ON b.id = p.build_id WHERE p.id = ?",
    )
    .bind(product_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let target_val: Option<f64> = form.target.trim().parse().ok();
    let current_val: Option<f64> = form.current.trim().parse().ok();
    let _ = crate::db::pg_query(
        "INSERT INTO lean_measure (product_id, metric, target, current, notes, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(product_id)
    .bind(form.metric.trim())
    .bind(target_val)
    .bind(current_val)
    .bind(form.notes.trim())
    .execute(&state.db)
    .await;
    session.push_flash("success", "Measure ditambahkan.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn measure_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = crate::db::pg_query(
        "SELECT m.id, m.metric, m.target, m.current, m.notes, b.idea_id
         FROM lean_measure m
         JOIN lean_product p ON p.id = m.product_id
         JOIN lean_build b ON b.id = p.build_id
         WHERE m.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let idea_id: i64 = r.get("idea_id");
    let metric: String = r.get("metric");
    let mut ctx = Context::new();
    ctx.insert("title", &format!("Edit Measure: {}", metric));
    ctx.insert("item_type", "measure");
    ctx.insert(
        "item",
        &serde_json::json!({
            "id": pk,
            "metric": metric,
            "target": r.get::<Option<f64>, _>("target"),
            "current": r.get::<Option<f64>, _>("current"),
            "notes": r.get::<String, _>("notes"),
        }),
    );
    ctx.insert("back_url", &format!("/lean/idea/{}/", idea_id));
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn measure_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<MeasureForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar(
        "SELECT b.idea_id FROM lean_measure m JOIN lean_product p ON p.id = m.product_id JOIN lean_build b ON b.id = p.build_id WHERE m.id = ?",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let target_val: Option<f64> = form.target.trim().parse().ok();
    let current_val: Option<f64> = form.current.trim().parse().ok();
    let _ = crate::db::pg_query(
        "UPDATE lean_measure SET metric = ?, target = ?, current = ?, notes = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(form.metric.trim())
    .bind(target_val)
    .bind(current_val)
    .bind(form.notes.trim())
    .bind(pk)
    .execute(&state.db)
    .await;
    session.push_flash("success", "Measure diperbarui.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn datapoint_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(measure_id): AxumPath<i64>,
    Form(form): Form<DataPointForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar(
        "SELECT b.idea_id FROM lean_measure m JOIN lean_product p ON p.id = m.product_id JOIN lean_build b ON b.id = p.build_id WHERE m.id = ?",
    )
    .bind(measure_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let dt = parse_dt_or_now(&form.measured_at);
    let _ = crate::db::pg_query(
        "INSERT INTO lean_datapoint (measure_id, value, measured_at, note, created_at, updated_at)
         VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(measure_id)
    .bind(form.value)
    .bind(dt)
    .bind(form.note.trim())
    .execute(&state.db)
    .await;
    session.push_flash("success", "DataPoint ditambahkan.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn datapoint_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = crate::db::pg_query(
        "SELECT d.id, d.value, d.measured_at, d.note, b.idea_id
         FROM lean_datapoint d
         JOIN lean_measure m ON m.id = d.measure_id
         JOIN lean_product p ON p.id = m.product_id
         JOIN lean_build b ON b.id = p.build_id
         WHERE d.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let idea_id: i64 = r.get("idea_id");
    let mdt: NaiveDateTime = r.get("measured_at");
    let mut ctx = Context::new();
    ctx.insert("title", "Edit DataPoint");
    ctx.insert("item_type", "datapoint");
    ctx.insert(
        "item",
        &serde_json::json!({
            "id": pk,
            "value": r.get::<f64, _>("value"),
            "measured_at": mdt.format("%Y-%m-%dT%H:%M").to_string(),
            "note": r.get::<String, _>("note"),
        }),
    );
    ctx.insert("back_url", &format!("/lean/idea/{}/", idea_id));
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn datapoint_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<DataPointForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> = crate::db::pg_query_scalar(
        "SELECT b.idea_id FROM lean_datapoint d JOIN lean_measure m ON m.id = d.measure_id JOIN lean_product p ON p.id = m.product_id JOIN lean_build b ON b.id = p.build_id WHERE d.id = ?",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let dt = parse_dt_or_now(&form.measured_at);
    let _ = crate::db::pg_query(
        "UPDATE lean_datapoint SET value = ?, measured_at = ?, note = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(form.value)
    .bind(dt)
    .bind(form.note.trim())
    .bind(pk)
    .execute(&state.db)
    .await;
    session.push_flash("success", "DataPoint diperbarui.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn learning_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(idea_id): AxumPath<i64>,
    Form(form): Form<LearningForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let _ = crate::db::pg_query(
        "INSERT INTO lean_learning (idea_id, summary, action, created_at, updated_at)
         VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(idea_id)
    .bind(form.summary.trim())
    .bind(form.action.trim())
    .execute(&state.db)
    .await;
    session.push_flash("success", "Learning ditambahkan.");
    redirect_with_session(&state, &session.payload, &format!("/lean/idea/{}/", idea_id))
}

pub async fn learning_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = crate::db::pg_query(
        "SELECT id, idea_id, summary, action FROM lean_learning WHERE id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let Some(r) = row else {
        return Redirect::to("/lean/").into_response();
    };
    let idea_id: Option<i64> = r.get("idea_id");
    let mut ctx = Context::new();
    ctx.insert("title", "Edit Learning");
    ctx.insert("item_type", "learning");
    ctx.insert(
        "item",
        &serde_json::json!({
            "id": pk,
            "summary": r.get::<String, _>("summary"),
            "action": r.get::<String, _>("action"),
        }),
    );
    let back = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    ctx.insert("back_url", &back);
    render_page(&state, &session, "lean/item_form.html", ctx)
}

pub async fn learning_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Form(form): Form<LearningForm>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let idea_id: Option<i64> =
        crate::db::pg_query_scalar("SELECT idea_id FROM lean_learning WHERE id = ?")
            .bind(pk)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let _ = crate::db::pg_query(
        "UPDATE lean_learning SET summary = ?, action = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(form.summary.trim())
    .bind(form.action.trim())
    .bind(pk)
    .execute(&state.db)
    .await;
    session.push_flash("success", "Learning diperbarui.");
    let target = idea_id
        .map(|id| format!("/lean/idea/{}/", id))
        .unwrap_or_else(|| "/lean/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

fn parse_dt_or_now(raw: &str) -> NaiveDateTime {
    let trimmed = raw.trim();
    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
        })
        .unwrap_or_else(|_| Utc::now().naive_utc())
}

