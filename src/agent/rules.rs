use serde::{Deserialize, Serialize};

use crate::agent::config::AgentThresholds;

pub const INFO: &str = "info";
pub const WARNING: &str = "warning";
pub const CRITICAL: &str = "critical";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alert {
    pub severity: String,
    pub code: String,
    pub message: String,
}

impl Alert {
    pub fn new(severity: &str, code: &str, message: impl Into<String>) -> Self {
        Self {
            severity: severity.to_string(),
            code: code.to_string(),
            message: message.into(),
        }
    }
}

fn severity_rank(s: &str) -> u8 {
    match s {
        CRITICAL => 2,
        WARNING => 1,
        _ => 0,
    }
}

pub fn overall_status(alerts: &[Alert]) -> &'static str {
    let top = alerts.iter().map(|a| severity_rank(&a.severity)).max();
    match top {
        Some(2) => "red",
        Some(1) => "yellow",
        _ => "green",
    }
}

pub fn sort_alerts(mut alerts: Vec<Alert>) -> Vec<Alert> {
    alerts.sort_by(|a, b| severity_rank(&b.severity).cmp(&severity_rank(&a.severity)));
    alerts
}

pub fn format_comma_i64(val: i64) -> String {
    let neg = val < 0;
    let s = val.unsigned_abs().to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3 + 1);
    for (idx, ch) in s.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if neg {
        out.push('-');
    }
    out.chars().rev().collect()
}

pub fn money(val: f64) -> String {
    let rounded = val.round() as i64;
    format!("Rp {}", format_comma_i64(rounded))
}

#[derive(Clone, Debug, Serialize)]
pub struct PeriodTotals {
    pub revenue: f64,
    pub expenses: f64,
    pub net_profit: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExpenseChange {
    pub account: String,
    pub current: f64,
    pub previous: f64,
    pub delta: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnpostedSummary {
    pub count: usize,
    pub oldest_date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnbalancedItem {
    pub id: i64,
    pub date: String,
    pub date_fmt: String,
    pub description: String,
    pub period: String,
    pub is_posted: bool,
    pub total_debit: i64,
    pub total_credit: i64,
    pub difference: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct FinanceMetrics {
    pub month: String,
    pub today: String,
    pub days_left_in_month: i64,
    pub current: PeriodTotals,
    pub previous: PeriodTotals,
    pub cash: f64,
    pub open_period: String,
    pub unposted: UnpostedSummary,
    pub unbalanced: usize,
    pub unbalanced_items: Vec<UnbalancedItem>,
    pub expense_changes: Vec<ExpenseChange>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KanbanColumnCount {
    pub name: String,
    pub open: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct KanbanTaskRow {
    pub id: i64,
    pub title: String,
    pub priority: String,
    pub priority_title: String,
    pub due_date: Option<String>,
    pub due_date_fmt: String,
    pub due_date_short: String,
    pub column: String,
    pub board_id: i64,
    pub board_name: String,
    pub days_overdue: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct KanbanBoardMetrics {
    pub board_id: i64,
    pub board: String,
    pub open_total: usize,
    pub overdue: usize,
    pub urgent_overdue: usize,
    pub due_soon: usize,
    pub unassigned: usize,
    pub stalled: usize,
    pub completed_week: usize,
    pub columns: Vec<KanbanColumnCount>,
    pub top_tasks: Vec<KanbanTaskRow>,
    pub overdue_tasks: Vec<KanbanTaskRow>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeadRow {
    pub id: i64,
    pub name: String,
    pub company: String,
    pub status: String,
    pub priority: String,
    pub priority_title: String,
    pub estimated_value: f64,
    pub next_follow_up: Option<String>,
    pub next_follow_up_fmt: String,
    pub next_follow_up_short: String,
    pub assigned_to: Option<String>,
    pub days_overdue: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct LeadsMetrics {
    pub entity_id: String,
    pub entity_name: String,
    pub open_total: usize,
    pub pipeline_value: f64,
    pub overdue: usize,
    pub urgent_overdue: usize,
    pub due_today: usize,
    pub due_soon: usize,
    pub hot_count: usize,
    pub unassigned: usize,
    pub stalled: usize,
    pub stalled_hot: usize,
    pub new_month: usize,
    pub won_month: usize,
    pub top_leads: Vec<LeadRow>,
    pub overdue_leads: Vec<LeadRow>,
    pub stalled_leads: Vec<LeadRow>,
    pub unassigned_leads: Vec<LeadRow>,
}

pub fn finance_alerts(metrics: &FinanceMetrics, t: &AgentThresholds) -> Vec<Alert> {
    let mut alerts = Vec::new();
    let cur = &metrics.current;
    let prev = &metrics.previous;

    if metrics.unbalanced > 0 {
        alerts.push(Alert::new(
            CRITICAL,
            "unbalanced",
            format!(
                "{} journal entr{} unbalanced (debit does not equal credit).",
                metrics.unbalanced,
                if metrics.unbalanced == 1 {
                    "y is"
                } else {
                    "ies are"
                }
            ),
        ));
    }

    if metrics.cash < 0.0 {
        alerts.push(Alert::new(
            CRITICAL,
            "negative_cash",
            format!("Cash/bank position is negative ({}).", money(metrics.cash)),
        ));
    }

    if cur.net_profit < 0.0 {
        alerts.push(Alert::new(
            WARNING,
            "loss",
            format!(
                "Net loss of {} so far this month.",
                money(cur.net_profit.abs())
            ),
        ));
    }

    if prev.net_profit > 0.0 {
        let drop = (prev.net_profit - cur.net_profit) / prev.net_profit * 100.0;
        if drop >= t.profit_drop_percent && cur.net_profit >= 0.0 {
            alerts.push(Alert::new(
                WARNING,
                "profit_drop",
                format!("Net profit is down {:.0}% compared with last month.", drop),
            ));
        }
    }

    let unposted = metrics.unposted.count;
    if metrics.open_period < metrics.month {
        alerts.push(Alert::new(
            CRITICAL,
            "closing_overdue",
            format!(
                "Period {} is still open and overdue for closing.",
                metrics.open_period
            ),
        ));
    } else if unposted > 0 && metrics.days_left_in_month <= t.closing_warning_days {
        alerts.push(Alert::new(
            WARNING,
            "closing_soon",
            format!(
                "{} unposted journal(s); {} day(s) left before month end.",
                unposted, metrics.days_left_in_month
            ),
        ));
    } else if unposted > 0 {
        alerts.push(Alert::new(
            INFO,
            "unposted",
            format!("{} journal(s) are waiting to be posted.", unposted),
        ));
    }

    let mut shown = 0usize;
    for row in &metrics.expense_changes {
        if shown >= t.max_expense_alerts {
            break;
        }
        if row.delta < t.expense_jump_min_amount {
            continue;
        }
        let base = row.previous;
        let pct = if base > 0.0 {
            Some(row.delta / base * 100.0)
        } else {
            None
        };
        if pct.map(|p| p >= t.expense_jump_percent).unwrap_or(true) {
            let tail = match pct {
                Some(p) => format!(" (+{:.0}%)", p),
                None => " (new this month)".to_string(),
            };
            alerts.push(Alert::new(
                WARNING,
                "expense_jump",
                format!(
                    "Expense '{}' rose by {}{}.",
                    row.account,
                    money(row.delta),
                    tail
                ),
            ));
            shown += 1;
        }
    }

    sort_alerts(alerts)
}

pub fn kanban_alerts(m: &KanbanBoardMetrics, t: &AgentThresholds) -> Vec<Alert> {
    let mut alerts = Vec::new();
    if m.overdue > 0 {
        let critical = m.urgent_overdue > 0 || m.overdue >= t.overdue_critical_count;
        alerts.push(Alert::new(
            if critical { CRITICAL } else { WARNING },
            "overdue",
            format!("{} task(s) overdue.", m.overdue),
        ));
    }
    if m.stalled > 0 {
        alerts.push(Alert::new(
            WARNING,
            "stalled",
            format!(
                "{} task(s) not updated for {}+ days.",
                m.stalled, t.stalled_days
            ),
        ));
    }
    if m.due_soon > 0 {
        alerts.push(Alert::new(
            INFO,
            "due_soon",
            format!(
                "{} task(s) due within {} days.",
                m.due_soon, t.due_soon_days
            ),
        ));
    }
    if m.unassigned > 0 {
        alerts.push(Alert::new(
            INFO,
            "unassigned",
            format!("{} open task(s) have no assignee.", m.unassigned),
        ));
    }
    sort_alerts(alerts)
}

pub fn leads_alerts(m: &LeadsMetrics, t: &AgentThresholds) -> Vec<Alert> {
    let mut alerts = Vec::new();
    if m.overdue > 0 {
        let critical = m.urgent_overdue > 0 || m.overdue >= t.lead_overdue_critical_count;
        let mut msg = format!("{} lead(s) overdue for follow-up", m.overdue);
        if m.urgent_overdue > 0 {
            msg.push_str(&format!(
                " (including {} hot lead{})",
                m.urgent_overdue,
                if m.urgent_overdue > 1 { "s" } else { "" }
            ));
        }
        msg.push('.');
        alerts.push(Alert::new(
            if critical { CRITICAL } else { WARNING },
            "lead_overdue",
            msg,
        ));
    }
    if m.stalled_hot > 0 {
        alerts.push(Alert::new(
            WARNING,
            "lead_stalled_hot",
            format!(
                "{} hot lead(s) with no interaction in {}+ days.",
                m.stalled_hot, t.lead_stalled_days
            ),
        ));
    } else if m.stalled > 0 {
        alerts.push(Alert::new(
            INFO,
            "lead_stalled",
            format!(
                "{} lead(s) with no interaction in {}+ days.",
                m.stalled, t.lead_stalled_days
            ),
        ));
    }
    if m.due_today > 0 {
        alerts.push(Alert::new(
            INFO,
            "lead_due_today",
            format!("{} lead(s) scheduled for follow-up today.", m.due_today),
        ));
    } else if m.due_soon > 0 {
        alerts.push(Alert::new(
            INFO,
            "lead_due_soon",
            format!(
                "{} lead(s) due for follow-up in the next {} days.",
                m.due_soon, t.lead_due_soon_days
            ),
        ));
    }
    if m.unassigned > 0 {
        let sev = if m.hot_count > 0 { WARNING } else { INFO };
        alerts.push(Alert::new(
            sev,
            "lead_unassigned",
            format!("{} open lead(s) have no assignee.", m.unassigned),
        ));
    }
    sort_alerts(alerts)
}

