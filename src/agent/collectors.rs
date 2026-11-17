use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::Row;

use crate::agent::config::AgentThresholds;
use crate::agent::llm::narrate;
use crate::agent::narrator::{narrate_board, narrate_finance, narrate_leads};
use crate::agent::rules::{
    finance_alerts, kanban_alerts, leads_alerts, overall_status, Alert, ExpenseChange,
    FinanceMetrics, KanbanBoardMetrics, KanbanColumnCount, KanbanTaskRow, LeadRow, LeadsMetrics,
    PeriodTotals, UnbalancedItem, UnpostedSummary,
};
use crate::models::{normalize_uuid, Entity};
use crate::routes::leads::status_label;
use crate::state::AppState;

fn priority_rank_kanban(p: &str) -> u8 {
    match p {
        "urgent" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 9,
    }
}

fn priority_rank_leads(p: &str) -> u8 {
    match p {
        "hot" => 0,
        "warm" => 1,
        "cold" => 2,
        _ => 9,
    }
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub fn month_bounds(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(day.year(), day.month(), 1).unwrap();
    let nxt = if start.month() == 12 {
        NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1).unwrap()
    };
    (start, nxt)
}

pub fn previous_month_start(day: NaiveDate) -> NaiveDate {
    let (start, _) = month_bounds(day);
    if start.month() == 1 {
        NaiveDate::from_ymd_opt(start.year() - 1, 12, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() - 1, 1).unwrap()
    }
}

fn days_in_month(year: i32, month: u32) -> i64 {
    let (_, nxt) = month_bounds(NaiveDate::from_ymd_opt(year, month, 1).unwrap());
    let last = nxt - Duration::days(1);
    last.day() as i64
}

pub async fn resolve_entities(state: &AppState, session_entity_id: &str) -> Vec<Entity> {
    let all = crate::db::pg_query_as::<Entity>(
        "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
         FROM entity_entity WHERE is_active = 1 ORDER BY name ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    if session_entity_id.is_empty() || session_entity_id == "all" {
        all
    } else {
        let norm = normalize_uuid(session_entity_id);
        all.into_iter()
            .filter(|e| normalize_uuid(&e.id) == norm)
            .collect()
    }
}

async fn sum_period_totals(
    state: &AppState,
    entity_id: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> PeriodTotals {
    let inc_row = crate::db::pg_query(
        "SELECT COALESCE(SUM(ji.debit), 0) AS d, COALESCE(SUM(ji.credit), 0) AS c
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE je.entity_id = ? AND je.date >= ? AND je.date < ? AND a.account_type = 'INCOME'",
    )
    .bind(entity_id)
    .bind(start)
    .bind(end)
    .fetch_one(&state.db)
    .await;

    let (inc_d, inc_c) = inc_row
        .map(|r| {
            (
                r.try_get::<i64, _>(0).unwrap_or(0) as f64,
                r.try_get::<i64, _>(1).unwrap_or(0) as f64,
            )
        })
        .unwrap_or((0.0, 0.0));

    let exp_row = crate::db::pg_query(
        "SELECT COALESCE(SUM(ji.debit), 0) AS d, COALESCE(SUM(ji.credit), 0) AS c
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE je.entity_id = ? AND je.date >= ? AND je.date < ? AND a.account_type IN ('EXPENSES', 'COGS')",
    )
    .bind(entity_id)
    .bind(start)
    .bind(end)
    .fetch_one(&state.db)
    .await;

    let (exp_d, exp_c) = exp_row
        .map(|r| {
            (
                r.try_get::<i64, _>(0).unwrap_or(0) as f64,
                r.try_get::<i64, _>(1).unwrap_or(0) as f64,
            )
        })
        .unwrap_or((0.0, 0.0));

    let revenue = inc_c - inc_d;
    let expenses = exp_d - exp_c;
    PeriodTotals {
        revenue,
        expenses,
        net_profit: revenue - expenses,
    }
}

async fn expense_by_account(
    state: &AppState,
    entity_id: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> HashMap<String, f64> {
    let rows = crate::db::pg_query(
        "SELECT a.account_name, COALESCE(SUM(ji.debit), 0) AS d, COALESCE(SUM(ji.credit), 0) AS c
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE je.entity_id = ? AND je.date >= ? AND je.date < ? AND a.account_type IN ('EXPENSES', 'COGS')
         GROUP BY a.account_name",
    )
    .bind(entity_id)
    .bind(start)
    .bind(end)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| {
            let name: String = r.get("account_name");
            let d = r.try_get::<i64, _>("d").unwrap_or(0) as f64;
            let c = r.try_get::<i64, _>("c").unwrap_or(0) as f64;
            (name, d - c)
        })
        .collect()
}

pub async fn collect_finance(state: &AppState, entity: &Entity, today: NaiveDate) -> FinanceMetrics {
    let (cur_start, cur_end) = month_bounds(today);
    let prev_start = previous_month_start(today);

    let current = sum_period_totals(state, &entity.id, cur_start, cur_end).await;
    let previous = sum_period_totals(state, &entity.id, prev_start, cur_start).await;

    let cur_exp = expense_by_account(state, &entity.id, cur_start, cur_end).await;
    let prev_exp = expense_by_account(state, &entity.id, prev_start, cur_start).await;
    let mut expense_changes: Vec<ExpenseChange> = cur_exp
        .into_iter()
        .map(|(account, amount)| {
            let before = *prev_exp.get(&account).unwrap_or(&0.0);
            ExpenseChange {
                account,
                current: amount,
                previous: before,
                delta: amount - before,
            }
        })
        .collect();
    expense_changes.sort_by(|a, b| b.delta.partial_cmp(&a.delta).unwrap_or(std::cmp::Ordering::Equal));

    let cash_row = crate::db::pg_query(
        "SELECT COALESCE(SUM(ji.debit), 0) AS d, COALESCE(SUM(ji.credit), 0) AS c
         FROM ledger_journalitem ji
         JOIN ledger_journalentry je ON je.id = ji.journal_entry_id
         JOIN ledger_account a ON a.id = ji.account_id
         WHERE je.entity_id = ? AND a.account_type = 'ASSET'
           AND (LOWER(a.account_name) LIKE '%cash%' OR LOWER(a.account_name) LIKE '%kas%' OR LOWER(a.account_name) LIKE '%bank%')",
    )
    .bind(&entity.id)
    .fetch_one(&state.db)
    .await;
    let cash = cash_row
        .map(|r| {
            (r.try_get::<i64, _>(0).unwrap_or(0) - r.try_get::<i64, _>(1).unwrap_or(0)) as f64
        })
        .unwrap_or(0.0);

    let open_period: String = crate::db::pg_query_scalar(
        "SELECT period FROM ledger_closingperiod WHERE entity_id = ? AND is_closed = 0 ORDER BY period DESC LIMIT 1",
    )
    .bind(&entity.id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| today.format("%Y-%m").to_string());

    let unposted_cnt: i64 = crate::db::pg_query_scalar(
        "SELECT COUNT(*) FROM ledger_journalentry WHERE entity_id = ? AND is_posted = 0",
    )
    .bind(&entity.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    let oldest_unposted: Option<NaiveDate> = crate::db::pg_query_scalar(
        "SELECT MIN(date) FROM ledger_journalentry WHERE entity_id = ? AND is_posted = 0",
    )
    .bind(&entity.id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let unbalanced_rows = crate::db::pg_query(
        "SELECT je.id, je.date, je.description, je.period, je.is_posted,
                COALESCE(SUM(ji.debit), 0) AS total_debit,
                COALESCE(SUM(ji.credit), 0) AS total_credit
         FROM ledger_journalentry je
         LEFT JOIN ledger_journalitem ji ON ji.journal_entry_id = je.id
         WHERE je.entity_id = ?
         GROUP BY je.id, je.date, je.description, je.period, je.is_posted
         HAVING total_debit != total_credit
         ORDER BY je.date DESC, je.id DESC",
    )
    .bind(&entity.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let unbalanced_total = unbalanced_rows.len();
    let unbalanced_items: Vec<UnbalancedItem> = unbalanced_rows
        .into_iter()
        .take(20)
        .map(|r| {
            let td: i64 = r.try_get("total_debit").unwrap_or(0);
            let tc: i64 = r.try_get("total_credit").unwrap_or(0);
            let dt: NaiveDate = r.get("date");
            let raw_desc: String = r.get("description");
            let desc = if raw_desc.trim().is_empty() {
                "No description".to_string()
            } else {
                raw_desc.trim().to_string()
            };
            UnbalancedItem {
                id: r.get("id"),
                date: dt.format("%Y-%m-%d").to_string(),
                date_fmt: dt.format("%d %b %Y").to_string(),
                description: desc,
                period: r.get::<Option<String>, _>("period").unwrap_or_default(),
                is_posted: r.get("is_posted"),
                total_debit: td,
                total_credit: tc,
                difference: td - tc,
            }
        })
        .collect();

    let last_day = days_in_month(today.year(), today.month());
    FinanceMetrics {
        month: today.format("%Y-%m").to_string(),
        today: today.format("%Y-%m-%d").to_string(),
        days_left_in_month: last_day - (today.day() as i64),
        current,
        previous,
        cash,
        open_period,
        unposted: UnpostedSummary {
            count: unposted_cnt.max(0) as usize,
            oldest_date: oldest_unposted.map(|d| d.format("%Y-%m-%d").to_string()),
        },
        unbalanced: unbalanced_total,
        unbalanced_items,
        expense_changes,
    }
}

pub async fn collect_board(
    state: &AppState,
    board_id: i64,
    board_name: &str,
    today: NaiveDate,
    t: &AgentThresholds,
) -> KanbanBoardMetrics {
    let now = Utc::now().naive_utc();
    let soon_limit = today + Duration::days(t.due_soon_days);
    let stalled_before = now - Duration::days(t.stalled_days);
    let week_ago = now - Duration::days(7);

    let open_rows = crate::db::pg_query(
        "SELECT t.id, t.title, t.priority, t.due_date, t.updated_at, c.name AS col_name,
                (SELECT COUNT(*) FROM kanban_task_assignees a WHERE a.task_id = t.id) AS assignee_cnt
         FROM kanban_task t
         JOIN kanban_column c ON c.id = t.column_id
         WHERE c.board_id = ? AND t.completed = 0",
    )
    .bind(board_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let open_total = open_rows.len();
    let mut overdue_list: Vec<(KanbanTaskRow, NaiveDate)> = Vec::new();
    let mut due_soon_list: Vec<(KanbanTaskRow, Option<NaiveDate>)> = Vec::new();
    let mut urgent_list: Vec<(KanbanTaskRow, Option<NaiveDate>)> = Vec::new();
    let mut unassigned = 0usize;
    let mut stalled = 0usize;

    for r in &open_rows {
        let tid: i64 = r.get("id");
        let title: String = r.get("title");
        let priority: String = r.get("priority");
        let due_date: Option<NaiveDate> = r.get("due_date");
        let updated_at: NaiveDateTime = r.get("updated_at");
        let col_name: String = r.get("col_name");
        let assignee_cnt: i64 = r.try_get("assignee_cnt").unwrap_or(0);

        if assignee_cnt == 0 {
            unassigned += 1;
        }
        if updated_at < stalled_before {
            stalled += 1;
        }

        let days_overdue = due_date
            .filter(|&d| d < today)
            .map(|d| (today - d).num_days())
            .unwrap_or(0);

        let row = KanbanTaskRow {
            id: tid,
            title,
            priority: priority.clone(),
            priority_title: title_case(&priority),
            due_date: due_date.map(|d| d.format("%Y-%m-%d").to_string()),
            due_date_fmt: due_date
                .map(|d| d.format("%d %b %Y").to_string())
                .unwrap_or_default(),
            due_date_short: due_date
                .map(|d| d.format("%d %b").to_string())
                .unwrap_or_default(),
            column: col_name,
            board_id,
            board_name: board_name.to_string(),
            days_overdue,
        };

        if let Some(dd) = due_date {
            if dd < today {
                overdue_list.push((row.clone(), dd));
            } else if dd <= soon_limit {
                due_soon_list.push((row.clone(), Some(dd)));
            }
        }
        if priority == "urgent" {
            urgent_list.push((row, due_date));
        }
    }

    overdue_list.sort_by(|a, b| {
        priority_rank_kanban(&a.0.priority)
            .cmp(&priority_rank_kanban(&b.0.priority))
            .then_with(|| a.1.cmp(&b.1))
    });
    let urgent_overdue = overdue_list
        .iter()
        .filter(|(r, _)| r.priority == "urgent")
        .count();

    let mut seen_ids = HashSet::new();
    let mut candidates: Vec<(KanbanTaskRow, Option<NaiveDate>)> = Vec::new();
    for (r, d) in &overdue_list {
        if seen_ids.insert(r.id) {
            candidates.push((r.clone(), Some(*d)));
        }
    }
    for (r, d) in &due_soon_list {
        if seen_ids.insert(r.id) {
            candidates.push((r.clone(), *d));
        }
    }
    for (r, d) in &urgent_list {
        if seen_ids.insert(r.id) {
            candidates.push((r.clone(), *d));
        }
    }
    candidates.sort_by(|a, b| {
        priority_rank_kanban(&a.0.priority)
            .cmp(&priority_rank_kanban(&b.0.priority))
            .then_with(|| a.1.is_none().cmp(&b.1.is_none()))
            .then_with(|| a.1.unwrap_or(today).cmp(&b.1.unwrap_or(today)))
    });
    let top_tasks: Vec<KanbanTaskRow> = candidates
        .into_iter()
        .take(t.top_priority_count)
        .map(|(r, _)| r)
        .collect();
    let overdue_tasks: Vec<KanbanTaskRow> = overdue_list.into_iter().map(|(r, _)| r).collect();

    let completed_week: i64 = crate::db::pg_query_scalar(
        "SELECT COUNT(*) FROM kanban_task t
         JOIN kanban_column c ON c.id = t.column_id
         WHERE c.board_id = ? AND t.completed = 1 AND t.updated_at >= ?",
    )
    .bind(board_id)
    .bind(week_ago)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let columns: Vec<KanbanColumnCount> = crate::db::pg_query(
        "SELECT c.name, COUNT(CASE WHEN t.completed = 0 THEN 1 END) AS open_cnt
         FROM kanban_column c
         LEFT JOIN kanban_task t ON t.column_id = c.id
         WHERE c.board_id = ?
         GROUP BY c.id, c.name, c.position
         ORDER BY c.position ASC",
    )
    .bind(board_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| KanbanColumnCount {
        name: r.get("name"),
        open: r.try_get::<i64, _>("open_cnt").unwrap_or(0).max(0) as usize,
    })
    .collect();

    KanbanBoardMetrics {
        board_id,
        board: board_name.to_string(),
        open_total,
        overdue: overdue_tasks.len(),
        urgent_overdue,
        due_soon: due_soon_list.len(),
        unassigned,
        stalled,
        completed_week: completed_week.max(0) as usize,
        columns,
        top_tasks,
        overdue_tasks,
    }
}

pub async fn collect_leads(
    state: &AppState,
    entity: &Entity,
    today: NaiveDate,
    t: &AgentThresholds,
) -> LeadsMetrics {
    let now = Utc::now().naive_utc();
    let month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
    let soon_limit = today + Duration::days(t.lead_due_soon_days);
    let stalled_before = now - Duration::days(t.lead_stalled_days);

    let open_rows = crate::db::pg_query(
        "SELECT l.id, l.name, l.company, l.status, l.priority, l.estimated_value, l.next_follow_up,
                l.created_at, u.username, u.first_name, u.last_name,
                (SELECT MAX(i.created_at) FROM leads_interaction i WHERE i.lead_id = l.id) AS last_int
         FROM leads_lead l
         LEFT JOIN auth_user u ON u.id = l.assigned_to_id
         WHERE l.entity_id = ? AND l.status NOT IN ('won', 'lost')",
    )
    .bind(&entity.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let open_total = open_rows.len();
    let mut pipeline_value = 0.0_f64;
    let mut overdue_list: Vec<(LeadRow, NaiveDate)> = Vec::new();
    let mut due_today_list: Vec<(LeadRow, Option<NaiveDate>)> = Vec::new();
    let mut due_soon_list: Vec<(LeadRow, Option<NaiveDate>)> = Vec::new();
    let mut hot_list: Vec<(LeadRow, Option<NaiveDate>)> = Vec::new();
    let mut stalled_list: Vec<(LeadRow, NaiveDateTime)> = Vec::new();
    let mut unassigned_list: Vec<(LeadRow, NaiveDateTime)> = Vec::new();
    let mut hot_count = 0usize;
    let mut stalled_hot = 0usize;

    for r in open_rows {
        let lid: i64 = r.get("id");
        let name: String = r.get("name");
        let company: String = r.get::<Option<String>, _>("company").unwrap_or_default();
        let status: String = r.get("status");
        let priority: String = r.get("priority");
        let est: Option<Decimal> = r.get("estimated_value");
        let est_f = est.and_then(|d| d.to_f64()).unwrap_or(0.0);
        pipeline_value += est_f;
        let next_fu: Option<NaiveDate> = r.get("next_follow_up");
        let created_at: NaiveDateTime = r.get("created_at");
        let last_int: Option<NaiveDateTime> = r.get("last_int");

        let uname: Option<String> = r.get("username");
        let assigned_to = uname.map(|u| {
            let f: String = r.get::<Option<String>, _>("first_name").unwrap_or_default();
            let l: String = r.get::<Option<String>, _>("last_name").unwrap_or_default();
            let full = format!("{} {}", f.trim(), l.trim()).trim().to_string();
            if full.is_empty() { u } else { full }
        });

        if priority == "hot" {
            hot_count += 1;
        }
        let is_stalled = match last_int {
            Some(li) => li < stalled_before,
            None => created_at < stalled_before,
        };
        if is_stalled && priority == "hot" {
            stalled_hot += 1;
        }

        let days_overdue = next_fu
            .filter(|&d| d < today)
            .map(|d| (today - d).num_days())
            .unwrap_or(0);

        let row = LeadRow {
            id: lid,
            name,
            company,
            status: status_label(&status).to_string(),
            priority: priority.clone(),
            priority_title: title_case(&priority),
            estimated_value: est_f,
            next_follow_up: next_fu.map(|d| d.format("%Y-%m-%d").to_string()),
            next_follow_up_fmt: next_fu
                .map(|d| d.format("%d %b %Y").to_string())
                .unwrap_or_default(),
            next_follow_up_short: next_fu
                .map(|d| d.format("%d %b").to_string())
                .unwrap_or_default(),
            assigned_to: assigned_to.clone(),
            days_overdue,
        };

        if is_stalled {
            stalled_list.push((row.clone(), created_at));
        }
        if assigned_to.is_none() {
            unassigned_list.push((row.clone(), created_at));
        }
        if let Some(dd) = next_fu {
            if dd < today {
                overdue_list.push((row.clone(), dd));
            } else if dd == today {
                due_today_list.push((row.clone(), Some(dd)));
            } else if dd <= soon_limit {
                due_soon_list.push((row.clone(), Some(dd)));
            }
        }
        if priority == "hot" {
            hot_list.push((row, next_fu));
        }
    }

    let urgent_overdue = overdue_list
        .iter()
        .filter(|(r, _)| r.priority == "hot")
        .count();

    let mut seen = HashSet::new();
    let mut candidates: Vec<(LeadRow, Option<NaiveDate>)> = Vec::new();
    for (r, d) in &overdue_list {
        if seen.insert(r.id) {
            candidates.push((r.clone(), Some(*d)));
        }
    }
    for (r, d) in &due_today_list {
        if seen.insert(r.id) {
            candidates.push((r.clone(), *d));
        }
    }
    for (r, d) in &due_soon_list {
        if seen.insert(r.id) {
            candidates.push((r.clone(), *d));
        }
    }
    for (r, d) in &hot_list {
        if seen.insert(r.id) {
            candidates.push((r.clone(), *d));
        }
    }

    candidates.sort_by(|a, b| {
        priority_rank_leads(&a.0.priority)
            .cmp(&priority_rank_leads(&b.0.priority))
            .then_with(|| a.1.is_none().cmp(&b.1.is_none()))
            .then_with(|| a.1.unwrap_or(today).cmp(&b.1.unwrap_or(today)))
            .then_with(|| {
                b.0.estimated_value
                    .partial_cmp(&a.0.estimated_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let top_leads: Vec<LeadRow> = candidates
        .into_iter()
        .take(t.lead_top_priority_count)
        .map(|(r, _)| r)
        .collect();

    overdue_list.sort_by(|a, b| {
        priority_rank_leads(&a.0.priority)
            .cmp(&priority_rank_leads(&b.0.priority))
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| {
                b.0.estimated_value
                    .partial_cmp(&a.0.estimated_value)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let overdue_leads: Vec<LeadRow> = overdue_list.into_iter().map(|(r, _)| r).collect();

    stalled_list.sort_by(|a, b| b.1.cmp(&a.1));
    let stalled_count = stalled_list.len();
    let stalled_leads: Vec<LeadRow> = stalled_list.into_iter().take(10).map(|(r, _)| r).collect();

    unassigned_list.sort_by(|a, b| b.1.cmp(&a.1));
    let unassigned_count = unassigned_list.len();
    let unassigned_leads: Vec<LeadRow> = unassigned_list
        .into_iter()
        .take(10)
        .map(|(r, _)| r)
        .collect();

    let new_month: i64 = crate::db::pg_query_scalar(
        "SELECT COUNT(*) FROM leads_lead WHERE entity_id = ? AND DATE(created_at) >= ?",
    )
    .bind(&entity.id)
    .bind(month_start)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let won_month: i64 = crate::db::pg_query_scalar(
        "SELECT COUNT(*) FROM leads_lead WHERE entity_id = ? AND status = 'won' AND DATE(created_at) >= ?",
    )
    .bind(&entity.id)
    .bind(month_start)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    LeadsMetrics {
        entity_id: entity.id.clone(),
        entity_name: entity.name.clone(),
        open_total,
        pipeline_value,
        overdue: overdue_leads.len(),
        urgent_overdue,
        due_today: due_today_list.len(),
        due_soon: due_soon_list.len(),
        hot_count,
        unassigned: unassigned_count,
        stalled: stalled_count,
        stalled_hot,
        new_month: new_month.max(0) as usize,
        won_month: won_month.max(0) as usize,
        top_leads,
        overdue_leads,
        stalled_leads,
        unassigned_leads,
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct FinanceBriefing {
    pub entity: Entity,
    pub status: String,
    pub alerts: Vec<Alert>,
    pub metrics: FinanceMetrics,
    pub narration: String,
    pub narration_source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoardBriefing {
    pub board_id: i64,
    pub board_name: String,
    pub entity_name: Option<String>,
    pub status: String,
    pub alerts: Vec<Alert>,
    pub metrics: KanbanBoardMetrics,
    pub narration: String,
    pub narration_source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntityKanbanBriefing {
    pub entity: Entity,
    pub status: String,
    pub board_count: usize,
    pub total_open: usize,
    pub total_overdue: usize,
    pub total_urgent_overdue: usize,
    pub total_completed_week: usize,
    pub overdue_tasks: Vec<KanbanTaskRow>,
    pub boards: Vec<BoardBriefing>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeadsBriefing {
    pub entity: Entity,
    pub status: String,
    pub alerts: Vec<Alert>,
    pub metrics: LeadsMetrics,
    pub narration: String,
    pub narration_source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AllBriefings {
    pub finance: Vec<FinanceBriefing>,
    pub kanban: Vec<BoardBriefing>,
    pub kanban_by_entity: Vec<EntityKanbanBriefing>,
    pub leads: Vec<LeadsBriefing>,
}

fn status_order(s: &str) -> u8 {
    match s {
        "red" => 0,
        "yellow" => 1,
        _ => 2,
    }
}

pub async fn build_briefings(state: &AppState, session_entity_id: &str) -> AllBriefings {
    let today = Utc::now().date_naive();
    let t = AgentThresholds::default();
    let entities = resolve_entities(state, session_entity_id).await;

    let mut finance = Vec::new();
    let mut kanban = Vec::new();
    let mut kanban_by_entity = Vec::new();
    let mut leads = Vec::new();

    for e in &entities {
        let fm = collect_finance(state, e, today).await;
        let fa = finance_alerts(&fm, &t);
        let st = overall_status(&fa).to_string();
        let tpl = narrate_finance(&e.name, &fm, &fa);
        let (narration, narration_source) = narrate(state, &tpl, &fa).await;
        finance.push(FinanceBriefing {
            entity: e.clone(),
            status: st,
            alerts: fa,
            metrics: fm,
            narration,
            narration_source,
        });

        let boards = crate::db::pg_query("SELECT id, name FROM kanban_board WHERE entity_id = ? ORDER BY name ASC")
            .bind(&e.id)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

        if !boards.is_empty() {
            let mut entity_boards = Vec::new();
            let mut all_overdue = Vec::new();
            for br in boards {
                let bid: i64 = br.get("id");
                let bname: String = br.get("name");
                let bm = collect_board(state, bid, &bname, today, &t).await;
                let ba = kanban_alerts(&bm, &t);
                let bst = overall_status(&ba).to_string();
                let btpl = narrate_board(&bm, &ba);
                let (bnar, bsrc) = narrate(state, &btpl, &ba).await;
                all_overdue.extend(bm.overdue_tasks.clone());
                let bb = BoardBriefing {
                    board_id: bid,
                    board_name: bname,
                    entity_name: Some(e.name.clone()),
                    status: bst,
                    alerts: ba,
                    metrics: bm,
                    narration: bnar,
                    narration_source: bsrc,
                };
                kanban.push(bb.clone());
                entity_boards.push(bb);
            }
            all_overdue.sort_by(|a, b| {
                priority_rank_kanban(&a.priority)
                    .cmp(&priority_rank_kanban(&b.priority))
                    .then_with(|| b.days_overdue.cmp(&a.days_overdue))
            });
            let total_open = entity_boards.iter().map(|b| b.metrics.open_total).sum();
            let total_overdue = entity_boards.iter().map(|b| b.metrics.overdue).sum();
            let total_urgent_overdue = entity_boards.iter().map(|b| b.metrics.urgent_overdue).sum();
            let total_completed_week = entity_boards.iter().map(|b| b.metrics.completed_week).sum();
            let est = if entity_boards.iter().any(|b| b.status == "red") {
                "red"
            } else if entity_boards.iter().any(|b| b.status == "yellow") {
                "yellow"
            } else {
                "green"
            };
            kanban_by_entity.push(EntityKanbanBriefing {
                entity: e.clone(),
                status: est.to_string(),
                board_count: entity_boards.len(),
                total_open,
                total_overdue,
                total_urgent_overdue,
                total_completed_week,
                overdue_tasks: all_overdue,
                boards: entity_boards,
            });
        }

        let lm = collect_leads(state, e, today, &t).await;
        let la = leads_alerts(&lm, &t);
        let lst = overall_status(&la).to_string();
        let ltpl = narrate_leads(&e.name, &lm, &la);
        let (lnar, lsrc) = narrate(state, &ltpl, &la).await;
        leads.push(LeadsBriefing {
            entity: e.clone(),
            status: lst,
            alerts: la,
            metrics: lm,
            narration: lnar,
            narration_source: lsrc,
        });
    }

    finance.sort_by_key(|b| status_order(&b.status));
    kanban.sort_by_key(|b| status_order(&b.status));
    kanban_by_entity.sort_by_key(|b| status_order(&b.status));
    leads.sort_by_key(|b| status_order(&b.status));

    AllBriefings {
        finance,
        kanban,
        kanban_by_entity,
        leads,
    }
}

