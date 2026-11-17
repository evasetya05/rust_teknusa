use std::collections::HashMap;

use axum::extract::{Multipart, Path as AxumPath, Query, RawForm, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::Form;
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tera::Context;

use crate::auth::{redirect_with_session, render_page, SessionContext};
use crate::routes::entity::sanitize_filename;
use crate::state::AppState;

// =========================================================================
// Pure Calendar Grid Helpers (port of apps/kanban/calendar_utils.py)
// =========================================================================

pub const MAX_VISIBLE_PER_DAY: usize = 4;

#[derive(Clone, Debug)]
pub struct CalTaskInput {
    pub id: i64,
    pub title: String,
    pub priority: String,
    pub completed: bool,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub board_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CalTaskItem {
    pub id: i64,
    pub title: String,
    pub priority: String,
    pub completed: bool,
    pub overdue: bool,
    pub is_start: bool,
    pub is_end: bool,
    pub board_name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CalDayCell {
    pub date_iso: String,
    pub date_label: String,
    pub day_num: u32,
    pub in_month: bool,
    pub is_today: bool,
    pub tasks: Vec<CalTaskItem>,
    pub more: usize,
}

pub fn parse_month(value: Option<&str>, today: NaiveDate) -> (i32, u32) {
    if let Some(raw) = value {
        if let Some((ys, ms)) = raw.trim().split_once('-') {
            if let (Ok(y), Ok(m)) = (ys.parse::<i32>(), ms.parse::<u32>()) {
                if NaiveDate::from_ymd_opt(y, m, 1).is_some() {
                    return (y, m);
                }
            }
        }
    }
    (today.year(), today.month())
}

pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year * 12 + (month as i32 - 1) + delta;
    let ny = index.div_euclid(12);
    let nm = (index.rem_euclid(12) + 1) as u32;
    (ny, nm)
}

pub fn month_weeks(year: i32, month: u32) -> Vec<Vec<NaiveDate>> {
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let offset_from_mon = first.weekday().num_days_from_monday() as i64;
    let grid_start = first - Duration::days(offset_from_mon);

    let (ny, nm) = shift_month(year, month, 1);
    let last_of_month = NaiveDate::from_ymd_opt(ny, nm, 1).unwrap() - Duration::days(1);
    let offset_to_sun = (6 - last_of_month.weekday().num_days_from_monday()) as i64;
    let grid_end = last_of_month + Duration::days(offset_to_sun);

    let mut weeks = Vec::new();
    let mut cur = grid_start;
    while cur <= grid_end {
        let mut week = Vec::with_capacity(7);
        for _ in 0..7 {
            week.push(cur);
            cur += Duration::days(1);
        }
        weeks.push(week);
    }
    weeks
}

pub fn grid_range(year: i32, month: u32) -> (NaiveDate, NaiveDate) {
    let weeks = month_weeks(year, month);
    (weeks[0][0], weeks.last().unwrap()[6])
}

pub fn task_span(start: Option<NaiveDate>, due: Option<NaiveDate>) -> Option<(NaiveDate, NaiveDate)> {
    match (start, due) {
        (None, None) => None,
        (Some(s), None) => Some((s, s)),
        (None, Some(e)) => Some((e, e)),
        (Some(s), Some(e)) => {
            if s > e {
                Some((e, s))
            } else {
                Some((s, e))
            }
        }
    }
}

pub fn build_month_grid(
    year: i32,
    month: u32,
    tasks: &[CalTaskInput],
    today: NaiveDate,
) -> Vec<Vec<CalDayCell>> {
    let weeks = month_weeks(year, month);
    let grid_start = weeks[0][0];
    let grid_end = weeks.last().unwrap()[6];

    let mut by_day: HashMap<NaiveDate, Vec<CalTaskItem>> = HashMap::new();
    for task in tasks {
        let Some((start, end)) = task_span(task.start_date, task.due_date) else {
            continue;
        };
        if end < grid_start || start > grid_end {
            continue;
        }
        let mut day = start.max(grid_start);
        let last = end.min(grid_end);
        let overdue = !task.completed && task.due_date.map(|d| d < today).unwrap_or(false);
        while day <= last {
            by_day.entry(day).or_default().push(CalTaskItem {
                id: task.id,
                title: task.title.clone(),
                priority: task.priority.clone(),
                completed: task.completed,
                overdue,
                is_start: day == start,
                is_end: day == end,
                board_name: task.board_name.clone(),
            });
            day += Duration::days(1);
        }
    }

    weeks
        .into_iter()
        .map(|week| {
            week.into_iter()
                .map(|day| {
                    let mut items = by_day.remove(&day).unwrap_or_default();
                    items.sort_by(|a, b| {
                        a.completed
                            .cmp(&b.completed)
                            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                    });
                    let total = items.len();
                    let visible: Vec<CalTaskItem> =
                        items.into_iter().take(MAX_VISIBLE_PER_DAY).collect();
                    CalDayCell {
                        date_iso: day.format("%Y-%m-%d").to_string(),
                        date_label: day.format("%d %b").to_string(),
                        day_num: day.day(),
                        in_month: day.month() == month,
                        is_today: day == today,
                        tasks: visible,
                        more: total.saturating_sub(MAX_VISIBLE_PER_DAY),
                    }
                })
                .collect()
        })
        .collect()
}

// =========================================================================
// View Structs & Forms
// =========================================================================

#[derive(Clone, Debug, Serialize)]
pub struct BoardView {
    pub id: i64,
    pub entity_id: Option<String>,
    pub entity_name: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChecklistStats {
    pub total: usize,
    pub completed: usize,
    pub percent: usize,
    pub all_done: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct TaskCardView {
    pub id: i64,
    pub column_id: i64,
    pub board_id: i64,
    pub board_name: String,
    pub title: String,
    pub description: String,
    pub priority: String,
    pub start_date: String,
    pub due_date: String,
    pub due_date_short: String,
    pub completed: bool,
    pub assignees_count: usize,
    pub comments_count: usize,
    pub attachments_count: usize,
    pub checklist_stats: Option<ChecklistStats>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ColumnView {
    pub id: i64,
    pub name: String,
    pub position: u32,
    pub tasks: Vec<TaskCardView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChecklistItemView {
    pub id: i64,
    pub title: String,
    pub is_completed: bool,
    pub created_by: Option<String>,
    pub completed_by: Option<String>,
}

#[derive(Deserialize)]
pub struct BoardCreateForm {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Deserialize)]
pub struct CalendarQuery {
    pub month: Option<String>,
    pub board: Option<String>,
}

#[derive(Deserialize)]
pub struct TaskAddQuery {
    pub board: Option<i64>,
    pub due_date: Option<String>,
}

#[derive(Deserialize)]
pub struct MoveTaskForm {
    pub task_id: i64,
    pub column_id: i64,
}

#[derive(Deserialize)]
pub struct CommentForm {
    pub comment: String,
    #[serde(default)]
    pub next: String,
}

#[derive(Deserialize)]
pub struct ChecklistAddForm {
    pub title: String,
    #[serde(default)]
    pub ajax: String,
}

// =========================================================================
// Handlers
// =========================================================================

pub async fn board_list(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let boards = fetch_boards(&state, eid.as_deref()).await;
    let mut ctx = Context::new();
    ctx.insert("boards", &boards);
    render_page(&state, &session, "kanban/board_list.html", ctx)
}

pub async fn board_add_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    render_page(&state, &session, "kanban/board_form.html", Context::new())
}

pub async fn board_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<BoardCreateForm>,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    let eid = session.entity_filter_id();
    let res = sqlx::query(
        "INSERT INTO kanban_board (entity_id, name, description, created_by_id, created_at)
         VALUES (?, ?, ?, ?, NOW(6))",
    )
    .bind(eid)
    .bind(form.name.trim())
    .bind(form.description.trim())
    .bind(user.id)
    .execute(&state.db)
    .await;

    match res {
        Ok(ok) => {
            let board_id = ok.last_insert_id() as i64;
            for (pos, col_name) in [(1u32, "To Do"), (2u32, "In Progress"), (3u32, "Done")] {
                let _ = sqlx::query(
                    "INSERT INTO kanban_column (board_id, name, position) VALUES (?, ?, ?)",
                )
                .bind(board_id)
                .bind(col_name)
                .bind(pos)
                .execute(&state.db)
                .await;
            }
            session.push_flash("success", "Board berhasil dibuat.");
            redirect_with_session(
                &state,
                &session.payload,
                &format!("/kanban/board/{}/", board_id),
            )
        }
        Err(e) => {
            session.push_flash("error", format!("Gagal membuat board: {}", e));
            redirect_with_session(&state, &session.payload, "/kanban/")
        }
    }
}

pub async fn board_detail(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let Some(board) = fetch_one_board(&state, pk).await else {
        return Redirect::to("/kanban/").into_response();
    };

    let col_rows = sqlx::query(
        "SELECT id, name, position FROM kanban_column WHERE board_id = ? ORDER BY position ASC, id ASC",
    )
    .bind(pk)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut columns = Vec::new();
    for cr in col_rows {
        let col_id: i64 = cr.get("id");
        let task_rows = sqlx::query(
            "SELECT t.id, t.column_id, t.title, t.description, t.priority, t.start_date, t.due_date, t.completed,
                    (SELECT COUNT(*) FROM kanban_task_assignees a WHERE a.task_id = t.id) AS assignees_cnt,
                    (SELECT COUNT(*) FROM kanban_taskcomment cm WHERE cm.task_id = t.id) AS comments_cnt,
                    (SELECT COUNT(*) FROM kanban_taskattachment at WHERE at.task_id = t.id) AS attachments_cnt,
                    (SELECT COUNT(*) FROM kanban_taskchecklistitem ch WHERE ch.task_id = t.id) AS chk_total,
                    (SELECT COUNT(*) FROM kanban_taskchecklistitem ch WHERE ch.task_id = t.id AND ch.is_completed = 1) AS chk_done
             FROM kanban_task t
             WHERE t.column_id = ?
             ORDER BY t.position ASC, t.id ASC",
        )
        .bind(col_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let tasks: Vec<TaskCardView> = task_rows
            .into_iter()
            .map(|tr| {
                let sd: Option<NaiveDate> = tr.get("start_date");
                let dd: Option<NaiveDate> = tr.get("due_date");
                let chk_total: i64 = tr.try_get("chk_total").unwrap_or(0);
                let chk_done: i64 = tr.try_get("chk_done").unwrap_or(0);
                let checklist_stats = if chk_total > 0 {
                    let pct = ((chk_done as f64 / chk_total as f64) * 100.0) as usize;
                    Some(ChecklistStats {
                        total: chk_total as usize,
                        completed: chk_done as usize,
                        percent: pct,
                        all_done: chk_done == chk_total,
                    })
                } else {
                    None
                };

                TaskCardView {
                    id: tr.get("id"),
                    column_id: col_id,
                    board_id: pk,
                    board_name: board.name.clone(),
                    title: tr.get("title"),
                    description: tr.get("description"),
                    priority: tr.get("priority"),
                    start_date: sd.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
                    due_date: dd.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
                    due_date_short: dd.map(|d| d.format("%b %d").to_string()).unwrap_or_default(),
                    completed: tr.get("completed"),
                    assignees_count: tr.try_get::<i64, _>("assignees_cnt").unwrap_or(0) as usize,
                    comments_count: tr.try_get::<i64, _>("comments_cnt").unwrap_or(0) as usize,
                    attachments_count: tr.try_get::<i64, _>("attachments_cnt").unwrap_or(0) as usize,
                    checklist_stats,
                }
            })
            .collect();

        columns.push(ColumnView {
            id: col_id,
            name: cr.get("name"),
            position: cr.get("position"),
            tasks,
        });
    }

    let mut ctx = Context::new();
    ctx.insert("board", &board);
    ctx.insert("columns", &columns);
    render_page(&state, &session, "kanban/board_detail.html", ctx)
}

pub async fn kanban_calendar(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<CalendarQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let board_id = q.board.as_deref().and_then(|s| s.parse::<i64>().ok());
    render_calendar_view(&state, &session, q.month.as_deref(), board_id).await
}

pub async fn board_calendar(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    Query(q): Query<CalendarQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    render_calendar_view(&state, &session, q.month.as_deref(), Some(pk)).await
}

async fn render_calendar_view(
    state: &AppState,
    session: &SessionContext,
    month_param: Option<&str>,
    selected_board_id: Option<i64>,
) -> Response {
    let today = Utc::now().date_naive();
    let (year, month) = parse_month(month_param, today);
    let (grid_start, grid_end) = grid_range(year, month);
    let (prev_y, prev_m) = shift_month(year, month, -1);
    let (next_y, next_m) = shift_month(year, month, 1);

    let eid = session.entity_filter_id();
    let boards = fetch_boards(state, eid.as_deref()).await;
    let selected_board = selected_board_id
        .and_then(|bid| boards.iter().find(|b| b.id == bid).cloned());

    let scope_ids: Vec<i64> = if let Some(ref sb) = selected_board {
        vec![sb.id]
    } else {
        boards.iter().map(|b| b.id).collect()
    };

    let mut cal_tasks = Vec::new();
    let mut no_date_count = 0i64;

    if !scope_ids.is_empty() {
        let placeholders = scope_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let nd_sql = format!(
            "SELECT COUNT(*) FROM kanban_task t
             JOIN kanban_column c ON c.id = t.column_id
             WHERE c.board_id IN ({}) AND t.completed = 0 AND t.start_date IS NULL AND t.due_date IS NULL",
            placeholders
        );
        let mut q_nd = sqlx::query(&nd_sql);
        for id in &scope_ids {
            q_nd = q_nd.bind(id);
        }
        no_date_count = q_nd
            .fetch_one(&state.db)
            .await
            .map(|r| r.try_get::<i64, _>(0).unwrap_or(0))
            .unwrap_or(0);

        let t_sql = format!(
            "SELECT t.id, t.title, t.priority, t.completed, t.start_date, t.due_date, b.name AS board_name
             FROM kanban_task t
             JOIN kanban_column c ON c.id = t.column_id
             JOIN kanban_board b ON b.id = c.board_id
             WHERE c.board_id IN ({})
               AND NOT (t.start_date IS NULL AND t.due_date IS NULL)
               AND (t.start_date <= ? OR t.start_date IS NULL)
               AND (t.due_date >= ? OR t.due_date IS NULL)",
            placeholders
        );
        let mut q_t = sqlx::query(&t_sql);
        for id in &scope_ids {
            q_t = q_t.bind(id);
        }
        q_t = q_t.bind(grid_end).bind(grid_start);

        for r in q_t.fetch_all(&state.db).await.unwrap_or_default() {
            cal_tasks.push(CalTaskInput {
                id: r.get("id"),
                title: r.get("title"),
                priority: r.get("priority"),
                completed: r.get("completed"),
                start_date: r.get("start_date"),
                due_date: r.get("due_date"),
                board_name: r.get("board_name"),
            });
        }
    }

    let weeks = build_month_grid(year, month, &cal_tasks, today);
    let month_date = NaiveDate::from_ymd_opt(year, month, 1).unwrap();

    let mut ctx = Context::new();
    ctx.insert("boards", &boards);
    ctx.insert("selected_board", &selected_board);
    ctx.insert("weeks", &weeks);
    ctx.insert("month_label", &month_date.format("%B %Y").to_string());
    ctx.insert("current_month_val", &format!("{:04}-{:02}", year, month));
    ctx.insert("prev_month", &format!("{:04}-{:02}", prev_y, prev_m));
    ctx.insert("next_month", &format!("{:04}-{:02}", next_y, next_m));
    ctx.insert("today_month", &today.format("%Y-%m").to_string());
    ctx.insert("weekdays", &["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]);
    ctx.insert("no_date_count", &no_date_count);
    render_page(state, session, "kanban/board_calendar.html", ctx)
}

pub async fn task_add_get(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<TaskAddQuery>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let eid = session.entity_filter_id();
    let columns = fetch_columns_for_board_or_entity(&state, q.board, eid.as_deref()).await;
    let users = fetch_active_users(&state).await;
    let labels = fetch_labels(&state).await;

    let mut ctx = Context::new();
    ctx.insert("is_edit", &false);
    ctx.insert("columns", &columns);
    ctx.insert("users_list", &users);
    ctx.insert("labels_list", &labels);
    ctx.insert("default_due_date", &q.due_date.unwrap_or_default());
    ctx.insert("board_id_param", &q.board);
    render_page(&state, &session, "kanban/task_form.html", ctx)
}

pub async fn task_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    RawForm(raw): RawForm,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    let form = parse_multi_form(&raw);
    let title = first_val(&form, "title");
    let column_id: i64 = first_val(&form, "column").parse().unwrap_or(0);
    if title.is_empty() || column_id == 0 {
        session.push_flash("error", "Judul dan Kolom task wajib diisi.");
        return redirect_with_session(&state, &session.payload, "/kanban/");
    }
    let description = first_val(&form, "description");
    let priority = {
        let p = first_val(&form, "priority");
        if p.is_empty() { "medium".to_string() } else { p }
    };
    let start_date = NaiveDate::parse_from_str(&first_val(&form, "start_date"), "%Y-%m-%d").ok();
    let due_date = NaiveDate::parse_from_str(&first_val(&form, "due_date"), "%Y-%m-%d").ok();
    let completed = form.contains_key("completed");

    let res = sqlx::query(
        "INSERT INTO kanban_task (column_id, title, description, priority, start_date, due_date, position, completed, created_by_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, NOW(6), NOW(6))",
    )
    .bind(column_id)
    .bind(&title)
    .bind(&description)
    .bind(&priority)
    .bind(start_date)
    .bind(due_date)
    .bind(completed)
    .bind(user.id)
    .execute(&state.db)
    .await;

    if let Ok(ok) = res {
        let task_id = ok.last_insert_id() as i64;
        sync_task_m2m(&state, task_id, &form).await;
        session.push_flash("success", "Task berhasil ditambahkan.");
        let board_id: Option<i64> =
            sqlx::query_scalar("SELECT board_id FROM kanban_column WHERE id = ?")
                .bind(column_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);
        let target = board_id
            .map(|b| format!("/kanban/board/{}/", b))
            .unwrap_or_else(|| "/kanban/".to_string());
        redirect_with_session(&state, &session.payload, &target)
    } else {
        session.push_flash("error", "Gagal membuat task.");
        redirect_with_session(&state, &session.payload, "/kanban/")
    }
}

pub async fn task_edit_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let row = sqlx::query(
        "SELECT t.id, t.column_id, c.board_id, b.name AS board_name, t.title, t.description,
                t.priority, t.start_date, t.due_date, t.completed
         FROM kanban_task t
         JOIN kanban_column c ON c.id = t.column_id
         JOIN kanban_board b ON b.id = c.board_id
         WHERE t.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let Some(r) = row else {
        return Redirect::to("/kanban/").into_response();
    };
    let board_id: i64 = r.get("board_id");
    let sd: Option<NaiveDate> = r.get("start_date");
    let dd: Option<NaiveDate> = r.get("due_date");

    let selected_assignees: Vec<i32> =
        sqlx::query_scalar("SELECT user_id FROM kanban_task_assignees WHERE task_id = ?")
            .bind(pk)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
    let selected_labels: Vec<i64> =
        sqlx::query_scalar("SELECT label_id FROM kanban_task_labels WHERE task_id = ?")
            .bind(pk)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let comments = fetch_task_comments(&state, pk).await;
    let attachments = fetch_task_attachments(&state, pk).await;
    let checklists = fetch_task_checklists(&state, pk).await;
    let pending: Vec<ChecklistItemView> = checklists
        .iter()
        .filter(|c| !c.is_completed)
        .cloned()
        .collect();
    let completed_items: Vec<ChecklistItemView> = checklists
        .iter()
        .filter(|c| c.is_completed)
        .cloned()
        .collect();
    let stats = compute_checklist_stats(&checklists);

    let mut ctx = Context::new();
    ctx.insert("is_edit", &true);
    ctx.insert(
        "object",
        &serde_json::json!({
            "id": pk,
            "column_id": r.get::<i64, _>("column_id"),
            "board_id": board_id,
            "board_name": r.get::<String, _>("board_name"),
            "title": r.get::<String, _>("title"),
            "description": r.get::<String, _>("description"),
            "priority": r.get::<String, _>("priority"),
            "start_date": sd.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
            "due_date": dd.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default(),
            "completed": r.get::<bool, _>("completed"),
        }),
    );
    ctx.insert(
        "columns",
        &fetch_columns_for_board_or_entity(&state, Some(board_id), None).await,
    );
    ctx.insert("users_list", &fetch_active_users(&state).await);
    ctx.insert("labels_list", &fetch_labels(&state).await);
    ctx.insert("selected_assignees", &selected_assignees);
    ctx.insert("selected_labels", &selected_labels);
    ctx.insert("comments", &comments);
    ctx.insert("attachments", &attachments);
    ctx.insert("checklists_pending", &pending);
    ctx.insert("checklists_completed", &completed_items);
    ctx.insert("checklist_stats", &stats);
    render_page(&state, &session, "kanban/task_form.html", ctx)
}

pub async fn task_edit_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
    RawForm(raw): RawForm,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let form = parse_multi_form(&raw);
    let title = first_val(&form, "title");
    let column_id: i64 = first_val(&form, "column").parse().unwrap_or(0);
    let description = first_val(&form, "description");
    let priority = {
        let p = first_val(&form, "priority");
        if p.is_empty() { "medium".to_string() } else { p }
    };
    let start_date = NaiveDate::parse_from_str(&first_val(&form, "start_date"), "%Y-%m-%d").ok();
    let due_date = NaiveDate::parse_from_str(&first_val(&form, "due_date"), "%Y-%m-%d").ok();
    let completed = form.contains_key("completed");

    let _ = sqlx::query(
        "UPDATE kanban_task
         SET column_id = ?, title = ?, description = ?, priority = ?, start_date = ?, due_date = ?, completed = ?, updated_at = NOW(6)
         WHERE id = ?",
    )
    .bind(column_id)
    .bind(&title)
    .bind(&description)
    .bind(&priority)
    .bind(start_date)
    .bind(due_date)
    .bind(completed)
    .bind(pk)
    .execute(&state.db)
    .await;

    sync_task_m2m(&state, pk, &form).await;
    session.push_flash("success", "Task diperbarui.");
    let board_id: Option<i64> =
        sqlx::query_scalar("SELECT board_id FROM kanban_column WHERE id = ?")
            .bind(column_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let target = board_id
        .map(|b| format!("/kanban/board/{}/", b))
        .unwrap_or_else(|| "/kanban/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn task_delete_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let board_id: Option<i64> = sqlx::query_scalar(
        "SELECT c.board_id FROM kanban_task t JOIN kanban_column c ON c.id = t.column_id WHERE t.id = ?",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);
    let _ = sqlx::query("DELETE FROM kanban_task WHERE id = ?")
        .bind(pk)
        .execute(&state.db)
        .await;
    session.push_flash("success", "Task dihapus.");
    let target = board_id
        .map(|b| format!("/kanban/board/{}/", b))
        .unwrap_or_else(|| "/kanban/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn task_move_post(
    State(state): State<AppState>,
    session: SessionContext,
    Form(form): Form<MoveTaskForm>,
) -> Response {
    if session.user.is_none() {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"ok": false}))).into_response();
    }
    let _ = sqlx::query(
        "UPDATE kanban_task SET column_id = ?, updated_at = NOW(6) WHERE id = ?",
    )
    .bind(form.column_id)
    .bind(form.task_id)
    .execute(&state.db)
    .await;
    Json(serde_json::json!({ "status": "success" })).into_response()
}

pub async fn task_comment_add_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(task_id): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let comments = fetch_task_comments(&state, task_id).await;
    let mut ctx = Context::new();
    ctx.insert("task_id", &task_id);
    ctx.insert("comments", &comments);
    render_page(&state, &session, "kanban/comment_form.html", ctx)
}

pub async fn task_comment_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(task_id): AxumPath<i64>,
    Form(form): Form<CommentForm>,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    if !form.comment.trim().is_empty() {
        let _ = sqlx::query(
            "INSERT INTO kanban_taskcomment (task_id, author_id, comment, created_at) VALUES (?, ?, ?, NOW(6))",
        )
        .bind(task_id)
        .bind(user.id)
        .bind(form.comment.trim())
        .execute(&state.db)
        .await;
        session.push_flash("success", "Komentar ditambahkan.");
    }
    let next = if form.next.starts_with('/') {
        form.next
    } else {
        format!("/kanban/task/{}/edit/", task_id)
    };
    redirect_with_session(&state, &session.payload, &next)
}

pub async fn task_comment_delete_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let task_id: Option<i64> =
        sqlx::query_scalar("SELECT task_id FROM kanban_taskcomment WHERE id = ?")
            .bind(pk)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let _ = sqlx::query("DELETE FROM kanban_taskcomment WHERE id = ?")
        .bind(pk)
        .execute(&state.db)
        .await;
    session.push_flash("success", "Komentar dihapus.");
    let target = task_id
        .map(|tid| format!("/kanban/task/{}/edit/", tid))
        .unwrap_or_else(|| "/kanban/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn task_attachment_add_get(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(task_id): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let attachments = fetch_task_attachments(&state, task_id).await;
    let mut ctx = Context::new();
    ctx.insert("task_id", &task_id);
    ctx.insert("attachments", &attachments);
    render_page(&state, &session, "kanban/attachment_form.html", ctx)
}

pub async fn task_attachment_add_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(task_id): AxumPath<i64>,
    mut multipart: Multipart,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    let mut next_url = format!("/kanban/task/{}/edit/", task_id);
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "next" {
            if let Ok(v) = field.text().await {
                if v.starts_with('/') {
                    next_url = v;
                }
            }
        } else if name == "file" {
            let fname = field.file_name().unwrap_or("").to_string();
            if !fname.is_empty() {
                if let Ok(bytes) = field.bytes().await {
                    let safe = sanitize_filename(&fname);
                    let rel = format!(
                        "kanban/tasks/{}_{}",
                        Utc::now().timestamp_millis(),
                        safe
                    );
                    let dest = state.config.media_root.join(&rel);
                    if let Some(parent) = dest.parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    if tokio::fs::write(&dest, &bytes).await.is_ok() {
                        let _ = sqlx::query(
                            "INSERT INTO kanban_taskattachment (task_id, file, uploaded_by_id, uploaded_at)
                             VALUES (?, ?, ?, NOW(6))",
                        )
                        .bind(task_id)
                        .bind(&rel)
                        .bind(user.id)
                        .execute(&state.db)
                        .await;
                        session.push_flash("success", "Lampiran diunggah.");
                    }
                }
            }
        }
    }
    redirect_with_session(&state, &session.payload, &next_url)
}

pub async fn task_attachment_delete_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let task_id: Option<i64> =
        sqlx::query_scalar("SELECT task_id FROM kanban_taskattachment WHERE id = ?")
            .bind(pk)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);
    let _ = sqlx::query("DELETE FROM kanban_taskattachment WHERE id = ?")
        .bind(pk)
        .execute(&state.db)
        .await;
    session.push_flash("success", "Lampiran dihapus.");
    let target = task_id
        .map(|tid| format!("/kanban/task/{}/edit/", tid))
        .unwrap_or_else(|| "/kanban/".to_string());
    redirect_with_session(&state, &session.payload, &target)
}

pub async fn task_checklist_add_post(
    State(state): State<AppState>,
    session: SessionContext,
    headers: HeaderMap,
    AxumPath(task_id): AxumPath<i64>,
    Form(form): Form<ChecklistAddForm>,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    let is_ajax = headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        == Some("XMLHttpRequest")
        || form.ajax == "1";
    let title = form.title.trim();
    if title.is_empty() {
        if is_ajax {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"ok": false, "error": "Title cannot be empty."})),
            )
                .into_response();
        }
        return Redirect::to(&format!("/kanban/task/{}/edit/", task_id)).into_response();
    }

    let pos: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kanban_taskchecklistitem WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let ok = sqlx::query(
        "INSERT INTO kanban_taskchecklistitem (task_id, title, is_completed, position, created_by_id, created_at)
         VALUES (?, ?, 0, ?, ?, NOW(6))",
    )
    .bind(task_id)
    .bind(title)
    .bind(pos as u32)
    .bind(user.id)
    .execute(&state.db)
    .await;

    let item_id = ok.map(|r| r.last_insert_id() as i64).unwrap_or(0);
    let items = fetch_task_checklists(&state, task_id).await;
    let stats = compute_checklist_stats(&items);

    if is_ajax {
        return Json(serde_json::json!({
            "ok": true,
            "item": {
                "id": item_id,
                "title": title,
                "is_completed": false,
                "created_by": user.display_name(),
            },
            "stats": stats,
        }))
        .into_response();
    }
    Redirect::to(&format!("/kanban/task/{}/edit/", task_id)).into_response()
}

pub async fn task_checklist_toggle_post(
    State(state): State<AppState>,
    session: SessionContext,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    let user = match session.require_auth_redirect() {
        Ok(u) => u.clone(),
        Err(resp) => return resp,
    };
    let row = sqlx::query("SELECT task_id, is_completed FROM kanban_taskchecklistitem WHERE id = ? LIMIT 1")
        .bind(pk)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let Some(r) = row else {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({"ok": false}))).into_response();
    };
    let task_id: i64 = r.get("task_id");
    let was_done: bool = r.get("is_completed");
    let now_done = !was_done;

    if now_done {
        let _ = sqlx::query(
            "UPDATE kanban_taskchecklistitem SET is_completed = 1, completed_at = NOW(6), completed_by_id = ? WHERE id = ?",
        )
        .bind(user.id)
        .bind(pk)
        .execute(&state.db)
        .await;
    } else {
        let _ = sqlx::query(
            "UPDATE kanban_taskchecklistitem SET is_completed = 0, completed_at = NULL, completed_by_id = NULL WHERE id = ?",
        )
        .bind(pk)
        .execute(&state.db)
        .await;
    }

    let items = fetch_task_checklists(&state, task_id).await;
    let stats = compute_checklist_stats(&items);
    Json(serde_json::json!({
        "ok": true,
        "id": pk,
        "is_completed": now_done,
        "stats": stats,
    }))
    .into_response()
}

pub async fn task_checklist_delete_post(
    State(state): State<AppState>,
    session: SessionContext,
    headers: HeaderMap,
    AxumPath(pk): AxumPath<i64>,
) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }
    let task_id: Option<i64> =
        sqlx::query_scalar("SELECT task_id FROM kanban_taskchecklistitem WHERE id = ?")
            .bind(pk)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    let _ = sqlx::query("DELETE FROM kanban_taskchecklistitem WHERE id = ?")
        .bind(pk)
        .execute(&state.db)
        .await;

    let tid = task_id.unwrap_or(0);
    let items = fetch_task_checklists(&state, tid).await;
    let stats = compute_checklist_stats(&items);

    let is_ajax = headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        == Some("XMLHttpRequest");
    if is_ajax {
        return Json(serde_json::json!({ "ok": true, "stats": stats })).into_response();
    }
    Redirect::to(&format!("/kanban/task/{}/edit/", tid)).into_response()
}

// --- DB Helpers ---

async fn fetch_boards(state: &AppState, eid: Option<&str>) -> Vec<BoardView> {
    let mut sql = String::from(
        "SELECT b.id, b.entity_id, e.name AS entity_name, b.name, b.description, b.created_at
         FROM kanban_board b
         LEFT JOIN entity_entity e ON e.id = b.entity_id
         WHERE 1=1",
    );
    if eid.is_some() {
        sql.push_str(" AND b.entity_id = ?");
    }
    sql.push_str(" ORDER BY b.name ASC");

    let mut q = sqlx::query(&sql);
    if let Some(e) = eid {
        q = q.bind(e);
    }
    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let dt: NaiveDateTime = r.get("created_at");
            BoardView {
                id: r.get("id"),
                entity_id: r.get("entity_id"),
                entity_name: r.get::<Option<String>, _>("entity_name").unwrap_or_default(),
                name: r.get("name"),
                description: r.get("description"),
                created_at: dt.format("%d %b %Y").to_string(),
            }
        })
        .collect()
}

async fn fetch_one_board(state: &AppState, pk: i64) -> Option<BoardView> {
    let r = sqlx::query(
        "SELECT b.id, b.entity_id, e.name AS entity_name, b.name, b.description, b.created_at
         FROM kanban_board b
         LEFT JOIN entity_entity e ON e.id = b.entity_id
         WHERE b.id = ? LIMIT 1",
    )
    .bind(pk)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)?;
    let dt: NaiveDateTime = r.get("created_at");
    Some(BoardView {
        id: r.get("id"),
        entity_id: r.get("entity_id"),
        entity_name: r.get::<Option<String>, _>("entity_name").unwrap_or_default(),
        name: r.get("name"),
        description: r.get("description"),
        created_at: dt.format("%d %b %Y").to_string(),
    })
}

async fn fetch_columns_for_board_or_entity(
    state: &AppState,
    board_id: Option<i64>,
    eid: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut sql = String::from(
        "SELECT c.id, c.name, b.name AS board_name
         FROM kanban_column c
         JOIN kanban_board b ON b.id = c.board_id
         WHERE 1=1",
    );
    if board_id.is_some() {
        sql.push_str(" AND c.board_id = ?");
    } else if eid.is_some() {
        sql.push_str(" AND b.entity_id = ?");
    }
    sql.push_str(" ORDER BY b.name ASC, c.position ASC");

    let mut q = sqlx::query(&sql);
    if let Some(bid) = board_id {
        q = q.bind(bid);
    } else if let Some(e) = eid {
        q = q.bind(e);
    }
    q.fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "name": r.get::<String, _>("name"),
                "board_name": r.get::<String, _>("board_name"),
            })
        })
        .collect()
}

async fn fetch_active_users(state: &AppState) -> Vec<serde_json::Value> {
    sqlx::query("SELECT id, username, first_name, last_name FROM auth_user WHERE is_active = 1 ORDER BY username")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            let u: String = r.get("username");
            let f: String = r.get("first_name");
            let l: String = r.get("last_name");
            let full = format!("{} {}", f.trim(), l.trim()).trim().to_string();
            serde_json::json!({
                "id": r.get::<i32, _>("id"),
                "name": if full.is_empty() { u } else { full },
            })
        })
        .collect()
}

async fn fetch_labels(state: &AppState) -> Vec<serde_json::Value> {
    sqlx::query("SELECT id, name, color FROM kanban_label ORDER BY name")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "name": r.get::<String, _>("name"),
                "color": r.get::<String, _>("color"),
            })
        })
        .collect()
}

async fn sync_task_m2m(state: &AppState, task_id: i64, form: &HashMap<String, Vec<String>>) {
    let _ = sqlx::query("DELETE FROM kanban_task_assignees WHERE task_id = ?")
        .bind(task_id)
        .execute(&state.db)
        .await;
    if let Some(uids) = form.get("assignees") {
        for raw in uids {
            if let Ok(uid) = raw.trim().parse::<i32>() {
                let _ = sqlx::query(
                    "INSERT IGNORE INTO kanban_task_assignees (task_id, user_id) VALUES (?, ?)",
                )
                .bind(task_id)
                .bind(uid)
                .execute(&state.db)
                .await;
            }
        }
    }

    let _ = sqlx::query("DELETE FROM kanban_task_labels WHERE task_id = ?")
        .bind(task_id)
        .execute(&state.db)
        .await;
    if let Some(lids) = form.get("labels") {
        for raw in lids {
            if let Ok(lid) = raw.trim().parse::<i64>() {
                let _ = sqlx::query(
                    "INSERT IGNORE INTO kanban_task_labels (task_id, label_id) VALUES (?, ?)",
                )
                .bind(task_id)
                .bind(lid)
                .execute(&state.db)
                .await;
            }
        }
    }
}

async fn fetch_task_comments(state: &AppState, task_id: i64) -> Vec<serde_json::Value> {
    sqlx::query(
        "SELECT c.id, c.comment, c.created_at, u.username, u.first_name, u.last_name
         FROM kanban_taskcomment c
         JOIN auth_user u ON u.id = c.author_id
         WHERE c.task_id = ? ORDER BY c.created_at DESC",
    )
    .bind(task_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        let dt: NaiveDateTime = r.get("created_at");
        let uname: String = r.get("username");
        let f: String = r.get("first_name");
        let l: String = r.get("last_name");
        let full = format!("{} {}", f.trim(), l.trim()).trim().to_string();
        let author = if full.is_empty() { uname.clone() } else { full };
        let initials: String = uname.chars().take(2).collect::<String>().to_uppercase();
        serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "comment": r.get::<String, _>("comment"),
            "created_at": dt.format("%d %b %Y, %H:%M").to_string(),
            "author_name": author,
            "author_initials": initials,
        })
    })
    .collect()
}

async fn fetch_task_attachments(state: &AppState, task_id: i64) -> Vec<serde_json::Value> {
    sqlx::query(
        "SELECT a.id, a.file, a.uploaded_at, u.username
         FROM kanban_taskattachment a
         LEFT JOIN auth_user u ON u.id = a.uploaded_by_id
         WHERE a.task_id = ? ORDER BY a.uploaded_at DESC",
    )
    .bind(task_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        let dt: NaiveDateTime = r.get("uploaded_at");
        let file: String = r.get("file");
        let display_name = file.rsplit('/').next().unwrap_or(&file).to_string();
        serde_json::json!({
            "id": r.get::<i64, _>("id"),
            "file_url": format!("/media/{}", file),
            "file_name": display_name,
            "uploaded_at": dt.format("%d %b %Y %H:%M").to_string(),
            "uploaded_by": r.get::<Option<String>, _>("username").unwrap_or_else(|| "User".to_string()),
        })
    })
    .collect()
}

async fn fetch_task_checklists(state: &AppState, task_id: i64) -> Vec<ChecklistItemView> {
    sqlx::query(
        "SELECT ch.id, ch.title, ch.is_completed,
                uc.username AS c_user, uc.first_name AS c_fn, uc.last_name AS c_ln,
                ud.username AS d_user, ud.first_name AS d_fn, ud.last_name AS d_ln
         FROM kanban_taskchecklistitem ch
         LEFT JOIN auth_user uc ON uc.id = ch.created_by_id
         LEFT JOIN auth_user ud ON ud.id = ch.completed_by_id
         WHERE ch.task_id = ? ORDER BY ch.position ASC, ch.created_at ASC",
    )
    .bind(task_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| ChecklistItemView {
        id: r.get("id"),
        title: r.get("title"),
        is_completed: r.get("is_completed"),
        created_by: opt_display_name(r.get("c_user"), r.get("c_fn"), r.get("c_ln")),
        completed_by: opt_display_name(r.get("d_user"), r.get("d_fn"), r.get("d_ln")),
    })
    .collect()
}

fn opt_display_name(u: Option<String>, f: Option<String>, l: Option<String>) -> Option<String> {
    let uname = u?;
    let full = format!(
        "{} {}",
        f.as_deref().unwrap_or("").trim(),
        l.as_deref().unwrap_or("").trim()
    )
    .trim()
    .to_string();
    Some(if full.is_empty() { uname } else { full })
}

fn compute_checklist_stats(items: &[ChecklistItemView]) -> ChecklistStats {
    let total = items.len();
    let completed = items.iter().filter(|i| i.is_completed).count();
    let percent = if total > 0 {
        (completed * 100) / total
    } else {
        0
    };
    ChecklistStats {
        total,
        completed,
        percent,
        all_done: total > 0 && completed == total,
    }
}

fn parse_multi_form(raw: &[u8]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
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
        map.entry(kd).or_default().push(vd);
    }
    map
}

fn first_val(map: &HashMap<String, Vec<String>>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.first())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_shift_month() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        assert_eq!(parse_month(Some("2026-11"), today), (2026, 11));
        assert_eq!(parse_month(Some("bad"), today), (2026, 5));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
    }

    #[test]
    fn test_build_month_grid_spans_and_overdue() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap();
        let tasks = vec![
            CalTaskInput {
                id: 1,
                title: "Multi-day task".to_string(),
                priority: "high".to_string(),
                completed: false,
                start_date: NaiveDate::from_ymd_opt(2026, 5, 8),
                due_date: NaiveDate::from_ymd_opt(2026, 5, 9),
                board_name: "Ops".to_string(),
            },
            CalTaskInput {
                id: 2,
                title: "No date".to_string(),
                priority: "low".to_string(),
                completed: false,
                start_date: None,
                due_date: None,
                board_name: "Ops".to_string(),
            },
        ];
        let weeks = build_month_grid(2026, 5, &tasks, today);
        assert!(!weeks.is_empty());
        let may8 = weeks
            .iter()
            .flatten()
            .find(|d| d.date_iso == "2026-05-08")
            .unwrap();
        assert_eq!(may8.tasks.len(), 1);
        assert!(may8.tasks[0].is_start);
        assert!(!may8.tasks[0].is_end);
        assert!(may8.tasks[0].overdue);
    }
}

