use std::collections::{HashMap, HashSet};

use axum::extract::{Path as AxumPath, Query, Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::RequestExt;
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tera::Context;

use crate::auth::{render_page, SessionContext};
use crate::models::{normalize_uuid, Entity};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct GanttQuery {
    pub entity: Option<String>,
    pub board: Option<String>,
    pub done: Option<String>,
    pub nodate: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct GanttBoardOption {
    pub id: i64,
    pub name: String,
    pub entity_name: Option<String>,
}

fn priority_color(priority: &str) -> &'static str {
    match priority {
        "low" => "#94a3b8",
        "medium" => "#3b82f6",
        "high" => "#f59e0b",
        "urgent" => "#ef4444",
        _ => "#3b82f6",
    }
}

fn resolve_dates(start: Option<NaiveDate>, due: Option<NaiveDate>) -> (Option<NaiveDate>, Option<NaiveDate>) {
    let mut s = start.or(due);
    let mut e = due.or(start);
    if let (Some(sv), Some(ev)) = (s, e) {
        if ev < sv {
            s = Some(ev);
            e = Some(sv);
        }
    }
    (s, e)
}

pub fn reaches_in_graph(adj: &HashMap<i64, Vec<i64>>, start_id: i64, target_id: i64) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![start_id];
    while let Some(cur) = stack.pop() {
        if cur == target_id {
            return true;
        }
        if !seen.insert(cur) {
            continue;
        }
        if let Some(neighbors) = adj.get(&cur) {
            for &nxt in neighbors {
                stack.push(nxt);
            }
        }
    }
    false
}

async fn reaches_in_db(state: &AppState, start_id: i64, target_id: i64) -> bool {
    let rows = sqlx::query("SELECT predecessor_id, successor_id FROM gantt_taskdependency")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for r in rows {
        let pred: i64 = r.get("predecessor_id");
        let succ: i64 = r.get("successor_id");
        adj.entry(pred).or_default().push(succ);
    }
    reaches_in_graph(&adj, start_id, target_id)
}

pub async fn gantt_view(
    State(state): State<AppState>,
    mut session: SessionContext,
    Query(q): Query<GanttQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let today = Utc::now().date_naive();
    let all_entities = sqlx::query_as::<_, Entity>(
        "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
         FROM entity_entity WHERE is_active = 1 ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut selected_entity_id = if let Some(ref ep) = q.entity {
        let trimmed = ep.trim();
        if trimmed.is_empty() {
            session.current_entity_id.clone()
        } else if trimmed == "all" {
            session.payload.current_entity_id = "all".to_string();
            "all".to_string()
        } else {
            let norm = normalize_uuid(trimmed);
            if let Some(found) = all_entities.iter().find(|e| normalize_uuid(&e.id) == norm) {
                session.payload.current_entity_id = found.id.clone();
                found.id.clone()
            } else {
                "all".to_string()
            }
        }
    } else {
        session.current_entity_id.clone()
    };

    let current_entity_obj = if selected_entity_id != "all" {
        let norm = normalize_uuid(&selected_entity_id);
        let f = all_entities
            .iter()
            .find(|e| normalize_uuid(&e.id) == norm)
            .cloned();
        if f.is_none() {
            selected_entity_id = "all".to_string();
        }
        f
    } else {
        None
    };

    let is_all_entities = selected_entity_id == "all";

    let mut b_sql = String::from(
        "SELECT b.id, b.name, e.name AS entity_name
         FROM kanban_board b
         LEFT JOIN entity_entity e ON e.id = b.entity_id
         WHERE 1=1",
    );
    if !is_all_entities {
        b_sql.push_str(" AND b.entity_id = ?");
    }
    b_sql.push_str(" ORDER BY e.name ASC, b.name ASC");
    let mut b_q = sqlx::query(&b_sql);
    if !is_all_entities {
        b_q = b_q.bind(&selected_entity_id);
    }
    let boards: Vec<GanttBoardOption> = b_q
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| GanttBoardOption {
            id: r.get("id"),
            name: r.get("name"),
            entity_name: r.get("entity_name"),
        })
        .collect();

    let selected_board_id = q
        .board
        .as_deref()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .filter(|bid| boards.iter().any(|b| b.id == *bid));
    let selected_board = selected_board_id.and_then(|bid| boards.iter().find(|b| b.id == bid).cloned());

    let show_done = q.done.as_deref() == Some("1");
    let show_nodate = q.nodate.as_deref() != Some("0");

    let scope_board_ids: Vec<i64> = if let Some(bid) = selected_board_id {
        vec![bid]
    } else {
        boards.iter().map(|b| b.id).collect()
    };

    let mut rows = Vec::new();
    let mut deps = Vec::new();
    let mut milestones = Vec::new();
    let mut all_tasks_for_arrows = Vec::new();
    let mut no_date_count = 0i64;

    if !scope_board_ids.is_empty() {
        let ph = scope_board_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let nd_sql = format!(
            "SELECT COUNT(*) FROM kanban_task t JOIN kanban_column c ON c.id = t.column_id
             WHERE c.board_id IN ({}) AND t.completed = 0 AND t.start_date IS NULL AND t.due_date IS NULL",
            ph
        );
        let mut nd_q = sqlx::query(&nd_sql);
        for id in &scope_board_ids {
            nd_q = nd_q.bind(id);
        }
        no_date_count = nd_q
            .fetch_one(&state.db)
            .await
            .map(|r| r.try_get::<i64, _>(0).unwrap_or(0))
            .unwrap_or(0);

        let mut t_sql = format!(
            "SELECT t.id, t.title, t.priority, t.start_date, t.due_date, t.completed,
                    c.name AS col_name, b.id AS board_id, b.name AS board_name,
                    b.entity_id, e.name AS entity_name,
                    bl.planned_start, bl.planned_end,
                    (SELECT COUNT(*) FROM kanban_taskchecklistitem ch WHERE ch.task_id = t.id) AS chk_total,
                    (SELECT COUNT(*) FROM kanban_taskchecklistitem ch WHERE ch.task_id = t.id AND ch.is_completed = 1) AS chk_done
             FROM kanban_task t
             JOIN kanban_column c ON c.id = t.column_id
             JOIN kanban_board b ON b.id = c.board_id
             LEFT JOIN entity_entity e ON e.id = b.entity_id
             LEFT JOIN gantt_taskbaseline bl ON bl.task_id = t.id
             WHERE c.board_id IN ({})",
            ph
        );
        if !show_done {
            t_sql.push_str(" AND t.completed = 0");
        }
        if !show_nodate {
            t_sql.push_str(" AND NOT (t.start_date IS NULL AND t.due_date IS NULL)");
        }
        t_sql.push_str(" ORDER BY e.name ASC, b.name ASC, t.start_date ASC, t.due_date ASC, t.id ASC");

        let mut t_q = sqlx::query(&t_sql);
        for id in &scope_board_ids {
            t_q = t_q.bind(id);
        }
        let task_records = t_q.fetch_all(&state.db).await.unwrap_or_default();

        let mut task_ids: Vec<i64> = Vec::new();
        for tr in task_records {
            let tid: i64 = tr.get("id");
            task_ids.push(tid);
            let sd: Option<NaiveDate> = tr.get("start_date");
            let dd: Option<NaiveDate> = tr.get("due_date");
            let (mut s, mut e) = resolve_dates(sd, dd);
            let mut is_nodate = false;
            if s.is_none() || e.is_none() {
                s = Some(today);
                e = Some(today);
                is_nodate = true;
            }
            let start = s.unwrap();
            let end = e.unwrap();
            let completed: bool = tr.get("completed");
            let priority: String = tr.get("priority");
            let chk_total: i64 = tr.try_get("chk_total").unwrap_or(0);
            let chk_done: i64 = tr.try_get("chk_done").unwrap_or(0);
            let progress = if completed {
                100
            } else if chk_total > 0 {
                (chk_done * 100) / chk_total
            } else {
                0
            };

            let assignees: Vec<String> = sqlx::query(
                "SELECT u.username, u.first_name, u.last_name
                 FROM kanban_task_assignees a
                 JOIN auth_user u ON u.id = a.user_id
                 WHERE a.task_id = ?",
            )
            .bind(tid)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|ur| {
                let f: String = ur.get("first_name");
                let l: String = ur.get("last_name");
                let full = format!("{} {}", f.trim(), l.trim()).trim().to_string();
                if full.is_empty() {
                    ur.get("username")
                } else {
                    full
                }
            })
            .collect();

            let bl_s: Option<NaiveDate> = tr.get("planned_start");
            let bl_e: Option<NaiveDate> = tr.get("planned_end");
            let baseline = match (bl_s, bl_e) {
                (Some(bs), Some(be)) => Some(serde_json::json!({
                    "start": bs.format("%Y-%m-%d").to_string(),
                    "end": be.format("%Y-%m-%d").to_string(),
                })),
                _ => None,
            };

            let entity_name: String = tr
                .get::<Option<String>, _>("entity_name")
                .unwrap_or_else(|| "Umum (Tanpa PT)".to_string());
            let entity_id_str: String = tr.get::<Option<String>, _>("entity_id").unwrap_or_default();

            rows.push(serde_json::json!({
                "id": tid,
                "title": tr.get::<String, _>("title"),
                "board": tr.get::<String, _>("board_name"),
                "board_id": tr.get::<i64, _>("board_id"),
                "entity": entity_name,
                "entity_id": entity_id_str,
                "column": tr.get::<String, _>("col_name"),
                "start": start.format("%Y-%m-%d").to_string(),
                "end": end.format("%Y-%m-%d").to_string(),
                "is_nodate": is_nodate,
                "priority": priority,
                "color": if is_nodate { "#94a3b8" } else { priority_color(&priority) },
                "completed": completed,
                "overdue": !completed && !is_nodate && end < today,
                "progress": progress,
                "assignees": assignees,
                "url": format!("/kanban/task/{}/edit/", tid),
                "baseline": baseline,
            }));
        }

        if !task_ids.is_empty() {
            let t_ph = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let dep_sql = format!(
                "SELECT id, predecessor_id, successor_id FROM gantt_taskdependency
                 WHERE predecessor_id IN ({0}) AND successor_id IN ({0})",
                t_ph
            );
            let mut dq = sqlx::query(&dep_sql);
            for id in &task_ids {
                dq = dq.bind(id);
            }
            for id in &task_ids {
                dq = dq.bind(id);
            }
            for dr in dq.fetch_all(&state.db).await.unwrap_or_default() {
                deps.push(serde_json::json!({
                    "id": dr.get::<i64, _>("id"),
                    "from": dr.get::<i64, _>("predecessor_id"),
                    "to": dr.get::<i64, _>("successor_id"),
                }));
            }
        }

        let ms_sql = format!(
            "SELECT m.id, m.title, m.date, m.is_done, m.board_id, b.name AS board_name,
                    m.task_id, t.title AS task_title, b.entity_id, e.name AS entity_name
             FROM gantt_milestone m
             JOIN kanban_board b ON b.id = m.board_id
             LEFT JOIN entity_entity e ON e.id = b.entity_id
             LEFT JOIN kanban_task t ON t.id = m.task_id
             WHERE m.board_id IN ({})
             ORDER BY m.date ASC, m.id ASC",
            ph
        );
        let mut mq = sqlx::query(&ms_sql);
        for id in &scope_board_ids {
            mq = mq.bind(id);
        }
        for mr in mq.fetch_all(&state.db).await.unwrap_or_default() {
            let mdate: NaiveDate = mr.get("date");
            milestones.push(serde_json::json!({
                "id": mr.get::<i64, _>("id"),
                "title": mr.get::<String, _>("title"),
                "date": mdate.format("%Y-%m-%d").to_string(),
                "done": mr.get::<bool, _>("is_done"),
                "board": mr.get::<String, _>("board_name"),
                "board_id": mr.get::<i64, _>("board_id"),
                "task_id": mr.get::<Option<i64>, _>("task_id"),
                "task_title": mr.get::<Option<String>, _>("task_title").unwrap_or_default(),
                "entity": mr.get::<Option<String>, _>("entity_name").unwrap_or_else(|| "Umum (Tanpa PT)".to_string()),
                "entity_id": mr.get::<Option<String>, _>("entity_id").unwrap_or_default(),
            }));
        }

        let mut all_sql = format!(
            "SELECT t.id, t.title, b.id AS board_id, b.name AS board_name, e.name AS entity_name
             FROM kanban_task t
             JOIN kanban_column c ON c.id = t.column_id
             JOIN kanban_board b ON b.id = c.board_id
             LEFT JOIN entity_entity e ON e.id = b.entity_id
             WHERE c.board_id IN ({})",
            ph
        );
        if !show_done {
            all_sql.push_str(" AND t.completed = 0");
        }
        all_sql.push_str(" ORDER BY b.name ASC, t.title ASC");
        let mut aq = sqlx::query(&all_sql);
        for id in &scope_board_ids {
            aq = aq.bind(id);
        }
        for ar in aq.fetch_all(&state.db).await.unwrap_or_default() {
            all_tasks_for_arrows.push(serde_json::json!({
                "id": ar.get::<i64, _>("id"),
                "title": ar.get::<String, _>("title"),
                "board": ar.get::<String, _>("board_name"),
                "board_id": ar.get::<i64, _>("board_id"),
                "entity": ar.get::<Option<String>, _>("entity_name").unwrap_or_else(|| "Umum (Tanpa PT)".to_string()),
            }));
        }
    }

    let data = serde_json::json!({
        "rows": rows,
        "deps": deps,
        "milestones": milestones,
        "all_tasks": all_tasks_for_arrows,
        "is_all_entities": is_all_entities,
    });

    let mut ctx = Context::new();
    ctx.insert("all_entities", &all_entities);
    ctx.insert("selected_entity_id", &selected_entity_id);
    ctx.insert("current_entity_obj", &current_entity_obj);
    ctx.insert("is_all_entities", &is_all_entities);
    ctx.insert("boards", &boards);
    ctx.insert("selected_board", &selected_board);
    ctx.insert("show_done", &show_done);
    ctx.insert("show_nodate", &show_nodate);
    ctx.insert("data_json", &data);
    ctx.insert("task_count", &rows.len());
    ctx.insert("milestone_count", &milestones.len());
    ctx.insert("no_date_count", &no_date_count);
    ctx.insert("today", &today.format("%Y-%m-%d").to_string());
    render_page(&state, &session, "gantt/gantt.html", ctx)
}

pub async fn update_task_dates(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(task_id): AxumPath<i64>,
    req: Request,
) -> Response {
    if session.user.is_none() {
        return json_err("Unauthorized", StatusCode::UNAUTHORIZED);
    }
    let params = extract_post_params(req).await;
    let start_s = params.get("start").map(|s| s.as_str()).unwrap_or("");
    let end_s = params.get("end").map(|s| s.as_str()).unwrap_or("");

    let (Ok(start), Ok(end)) = (
        NaiveDate::parse_from_str(start_s.trim(), "%Y-%m-%d"),
        NaiveDate::parse_from_str(end_s.trim(), "%Y-%m-%d"),
    ) else {
        return json_err("Data task atau tanggal tidak valid.", StatusCode::BAD_REQUEST);
    };

    if end < start {
        return json_err(
            "Tanggal selesai tidak boleh sebelum tanggal mulai.",
            StatusCode::BAD_REQUEST,
        );
    }

    let res = sqlx::query(
        "UPDATE kanban_task SET start_date = ?, due_date = ?, updated_at = NOW(6) WHERE id = ?",
    )
    .bind(start)
    .bind(end)
    .bind(task_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(ok) if ok.rows_affected() > 0 => Json(serde_json::json!({
            "ok": true,
            "start": start.format("%Y-%m-%d").to_string(),
            "end": end.format("%Y-%m-%d").to_string(),
        }))
        .into_response(),
        _ => json_err("Data task atau tanggal tidak valid.", StatusCode::BAD_REQUEST),
    }
}

pub async fn dependency_add(
    State(state): State<AppState>,
    session: SessionContext,
    req: Request,
) -> Response {
    let user = match session.user.as_ref() {
        Some(u) => u.clone(),
        None => return json_err("Unauthorized", StatusCode::UNAUTHORIZED),
    };
    let params = extract_post_params(req).await;
    let Ok(pred_id) = params.get("from").unwrap_or(&String::new()).trim().parse::<i64>() else {
        return json_err("ID task tidak valid.", StatusCode::BAD_REQUEST);
    };
    let Ok(succ_id) = params.get("to").unwrap_or(&String::new()).trim().parse::<i64>() else {
        return json_err("ID task tidak valid.", StatusCode::BAD_REQUEST);
    };

    if pred_id == succ_id {
        return json_err(
            "Task tidak dapat bergantung pada dirinya sendiri.",
            StatusCode::BAD_REQUEST,
        );
    }

    let pred_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM kanban_task WHERE id = ?")
        .bind(pred_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    let succ_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM kanban_task WHERE id = ?")
        .bind(succ_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    if pred_exists.is_none() || succ_exists.is_none() {
        return json_err("Task tidak ditemukan.", StatusCode::NOT_FOUND);
    }

    let dup: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM gantt_taskdependency WHERE predecessor_id = ? AND successor_id = ? LIMIT 1",
    )
    .bind(pred_id)
    .bind(succ_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    if dup.is_some() {
        return json_err(
            "Panah hubungan antar task ini sudah ada.",
            StatusCode::BAD_REQUEST,
        );
    }

    if reaches_in_db(&state, succ_id, pred_id).await {
        return json_err(
            "Tidak dapat membuat panah ini karena akan menyebabkan siklus (circular dependency).",
            StatusCode::BAD_REQUEST,
        );
    }

    let res = sqlx::query(
        "INSERT INTO gantt_taskdependency (predecessor_id, successor_id, created_by_id, created_at)
         VALUES (?, ?, ?, NOW(6))",
    )
    .bind(pred_id)
    .bind(succ_id)
    .bind(user.id)
    .execute(&state.db)
    .await;

    match res {
        Ok(ok) => Json(serde_json::json!({
            "ok": true,
            "id": ok.last_insert_id() as i64,
            "from": pred_id,
            "to": succ_id,
        }))
        .into_response(),
        Err(e) => json_err(&format!("Gagal menyimpan: {}", e), StatusCode::BAD_REQUEST),
    }
}

pub async fn dependency_delete(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if session.user.is_none() {
        return json_err("Unauthorized", StatusCode::UNAUTHORIZED);
    }
    let deleted = sqlx::query("DELETE FROM gantt_taskdependency WHERE id = ?")
        .bind(pk)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected() > 0)
        .unwrap_or(false);
    Json(serde_json::json!({ "ok": deleted })).into_response()
}

pub async fn milestone_add(
    State(state): State<AppState>,
    session: SessionContext,
    req: Request,
) -> Response {
    let user = match session.user.as_ref() {
        Some(u) => u.clone(),
        None => return json_err("Unauthorized", StatusCode::UNAUTHORIZED),
    };
    let params = extract_post_params(req).await;
    let mut title = params.get("title").map(|s| s.trim().to_string()).unwrap_or_default();
    let task_id_opt = params.get("task").and_then(|s| s.trim().parse::<i64>().ok());
    let board_id_opt = params.get("board").and_then(|s| s.trim().parse::<i64>().ok());
    let date_str = params.get("date").map(|s| s.trim().to_string()).unwrap_or_default();

    let (board_id, task_title, task_start, task_due) = if let Some(tid) = task_id_opt {
        let row = sqlx::query(
            "SELECT t.title, t.start_date, t.due_date, c.board_id
             FROM kanban_task t JOIN kanban_column c ON c.id = t.column_id WHERE t.id = ? LIMIT 1",
        )
        .bind(tid)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        let Some(r) = row else {
            return json_err("Task tidak ditemukan.", StatusCode::NOT_FOUND);
        };
        (
            r.get::<i64, _>("board_id"),
            Some(r.get::<String, _>("title")),
            r.get::<Option<NaiveDate>, _>("start_date"),
            r.get::<Option<NaiveDate>, _>("due_date"),
        )
    } else if let Some(bid) = board_id_opt {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM kanban_board WHERE id = ?")
            .bind(bid)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
        if exists.is_none() {
            return json_err("Board tidak ditemukan.", StatusCode::NOT_FOUND);
        }
        (bid, None, None, None)
    } else {
        return json_err("Pilih Board atau Task untuk milestone ini.", StatusCode::BAD_REQUEST);
    };

    let when = if date_str.is_empty() && task_id_opt.is_some() {
        task_due.or(task_start).unwrap_or_else(|| Utc::now().date_naive())
    } else {
        match NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => {
                return json_err(
                    "Format tanggal tidak valid (YYYY-MM-DD).",
                    StatusCode::BAD_REQUEST,
                )
            }
        }
    };

    if title.is_empty() {
        title = match task_title {
            Some(tt) => format!("Milestone: {}", tt),
            None => "Milestone".to_string(),
        };
    }
    let title_trunc: String = title.chars().take(200).collect();

    let res = sqlx::query(
        "INSERT INTO gantt_milestone (board_id, task_id, title, date, is_done, created_by_id, created_at)
         VALUES (?, ?, ?, ?, 0, ?, NOW(6))",
    )
    .bind(board_id)
    .bind(task_id_opt)
    .bind(&title_trunc)
    .bind(when)
    .bind(user.id)
    .execute(&state.db)
    .await;

    match res {
        Ok(ok) => Json(serde_json::json!({
            "ok": true,
            "id": ok.last_insert_id() as i64,
            "task_id": task_id_opt,
            "board_id": board_id,
            "title": title_trunc,
            "date": when.format("%Y-%m-%d").to_string(),
            "done": false,
        }))
        .into_response(),
        Err(e) => json_err(&format!("Gagal membuat milestone: {}", e), StatusCode::BAD_REQUEST),
    }
}

pub async fn milestone_toggle(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if session.user.is_none() {
        return json_err("Unauthorized", StatusCode::UNAUTHORIZED);
    }
    let cur: Option<bool> = sqlx::query_scalar("SELECT is_done FROM gantt_milestone WHERE id = ?")
        .bind(pk)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    let Some(was_done) = cur else {
        return json_err("Milestone tidak ditemukan.", StatusCode::NOT_FOUND);
    };
    let next = !was_done;
    let _ = sqlx::query("UPDATE gantt_milestone SET is_done = ? WHERE id = ?")
        .bind(next)
        .bind(pk)
        .execute(&state.db)
        .await;
    Json(serde_json::json!({ "ok": true, "done": next })).into_response()
}

pub async fn milestone_move(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    req: Request,
) -> Response {
    if session.user.is_none() {
        return json_err("Unauthorized", StatusCode::UNAUTHORIZED);
    }
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM gantt_milestone WHERE id = ?")
        .bind(pk)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
    if exists.is_none() {
        return json_err("Milestone tidak ditemukan.", StatusCode::NOT_FOUND);
    }
    let params = extract_post_params(req).await;
    let date_s = params.get("date").map(|s| s.trim()).unwrap_or("");
    let Ok(new_date) = NaiveDate::parse_from_str(date_s, "%Y-%m-%d") else {
        return json_err("Format tanggal tidak valid.", StatusCode::BAD_REQUEST);
    };
    let _ = sqlx::query("UPDATE gantt_milestone SET date = ? WHERE id = ?")
        .bind(new_date)
        .bind(pk)
        .execute(&state.db)
        .await;
    Json(serde_json::json!({
        "ok": true,
        "date": new_date.format("%Y-%m-%d").to_string(),
    }))
    .into_response()
}

pub async fn milestone_delete(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if session.user.is_none() {
        return json_err("Unauthorized", StatusCode::UNAUTHORIZED);
    }
    let deleted = sqlx::query("DELETE FROM gantt_milestone WHERE id = ?")
        .bind(pk)
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected() > 0)
        .unwrap_or(false);
    Json(serde_json::json!({ "ok": deleted })).into_response()
}

pub async fn baseline_set(
    State(state): State<AppState>,
    session: SessionContext,
    req: Request,
) -> Response {
    let user = match session.user.as_ref() {
        Some(u) => u.clone(),
        None => return json_err("Unauthorized", StatusCode::UNAUTHORIZED),
    };
    let params = extract_post_params(req).await;
    let overwrite = params.get("overwrite").map(|s| s == "1").unwrap_or(false);
    let board_id = params.get("board").and_then(|s| s.trim().parse::<i64>().ok());
    let eid = session.entity_filter_id();

    let mut sql = String::from(
        "SELECT t.id, t.start_date, t.due_date, bl.id AS bl_id
         FROM kanban_task t
         JOIN kanban_column c ON c.id = t.column_id
         JOIN kanban_board b ON b.id = c.board_id
         LEFT JOIN gantt_taskbaseline bl ON bl.task_id = t.id
         WHERE t.completed = 0 AND NOT (t.start_date IS NULL AND t.due_date IS NULL)",
    );
    if eid.is_some() {
        sql.push_str(" AND b.entity_id = ?");
    }
    if board_id.is_some() {
        sql.push_str(" AND b.id = ?");
    }

    let mut q = sqlx::query(&sql);
    if let Some(ref e) = eid {
        q = q.bind(e);
    }
    if let Some(bid) = board_id {
        q = q.bind(bid);
    }

    let mut count = 0usize;
    for r in q.fetch_all(&state.db).await.unwrap_or_default() {
        let tid: i64 = r.get("id");
        let existing_bl: Option<i64> = r.get("bl_id");
        if existing_bl.is_some() && !overwrite {
            continue;
        }
        let sd: Option<NaiveDate> = r.get("start_date");
        let dd: Option<NaiveDate> = r.get("due_date");
        let (Some(start), Some(end)) = resolve_dates(sd, dd) else {
            continue;
        };
        if existing_bl.is_some() {
            let _ = sqlx::query(
                "UPDATE gantt_taskbaseline SET planned_start = ?, planned_end = ?, set_by_id = ?, set_at = NOW(6) WHERE task_id = ?",
            )
            .bind(start)
            .bind(end)
            .bind(user.id)
            .bind(tid)
            .execute(&state.db)
            .await;
        } else {
            let _ = sqlx::query(
                "INSERT INTO gantt_taskbaseline (task_id, planned_start, planned_end, set_by_id, set_at)
                 VALUES (?, ?, ?, ?, NOW(6))",
            )
            .bind(tid)
            .bind(start)
            .bind(end)
            .bind(user.id)
            .execute(&state.db)
            .await;
        }
        count += 1;
    }

    Json(serde_json::json!({ "ok": true, "count": count })).into_response()
}

pub async fn baseline_clear(
    State(state): State<AppState>,
    session: SessionContext,
    req: Request,
) -> Response {
    if session.user.is_none() {
        return json_err("Unauthorized", StatusCode::UNAUTHORIZED);
    }
    let params = extract_post_params(req).await;
    let board_id = params.get("board").and_then(|s| s.trim().parse::<i64>().ok());
    let eid = session.entity_filter_id();

    let mut sql = String::from(
        "DELETE bl FROM gantt_taskbaseline bl
         JOIN kanban_task t ON t.id = bl.task_id
         JOIN kanban_column c ON c.id = t.column_id
         JOIN kanban_board b ON b.id = c.board_id
         WHERE 1=1",
    );
    if eid.is_some() {
        sql.push_str(" AND b.entity_id = ?");
    }
    if board_id.is_some() {
        sql.push_str(" AND b.id = ?");
    }
    let mut q = sqlx::query(&sql);
    if let Some(ref e) = eid {
        q = q.bind(e);
    }
    if let Some(bid) = board_id {
        q = q.bind(bid);
    }
    let deleted = q
        .execute(&state.db)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);

    Json(serde_json::json!({ "ok": true, "count": deleted })).into_response()
}

fn json_err(msg: &str, status: StatusCode) -> Response {
    (status, Json(serde_json::json!({ "ok": false, "error": msg }))).into_response()
}

/// Supports both `multipart/form-data` (JS `new FormData()`) and `application/x-www-form-urlencoded`.
async fn extract_post_params(req: Request) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let is_multipart = req
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.to_lowercase().starts_with("multipart/form-data"))
        .unwrap_or(false);

    if is_multipart {
        if let Ok(mut mp) = req.extract::<axum::extract::Multipart, _>().await {
            while let Ok(Some(field)) = mp.next_field().await {
                let name = field.name().unwrap_or("").to_string();
                if let Ok(val) = field.text().await {
                    out.insert(name, val);
                }
            }
        }
    } else if let Ok(body) = req.extract::<String, _>().await {
        for pair in body.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                let kd = urlencoding::decode(&k.replace('+', " "))
                    .unwrap_or_else(|_| k.into())
                    .into_owned();
                let vd = urlencoding::decode(&v.replace('+', " "))
                    .unwrap_or_else(|_| v.into())
                    .into_owned();
                out.insert(kd, vd);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gantt_cycle_detection() {
        let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
        adj.insert(1, vec![2]);
        adj.insert(2, vec![3]);
        // Adding 3 -> 1 would ask reaches(1, 3): true => cycle!
        assert!(reaches_in_graph(&adj, 1, 3));
        // Adding 1 -> 4 would ask reaches(4, 1): false => safe!
        assert!(!reaches_in_graph(&adj, 4, 1));
    }
}

