use crate::agent::rules::{
    money, Alert, FinanceMetrics, KanbanBoardMetrics, LeadsMetrics,
};

fn pct_change(current: f64, previous: f64) -> Option<f64> {
    if previous == 0.0 {
        None
    } else {
        Some((current - previous) / previous.abs() * 100.0)
    }
}

pub fn narrate_finance(entity_name: &str, metrics: &FinanceMetrics, alerts: &[Alert]) -> String {
    let cur = &metrics.current;
    let prev = &metrics.previous;
    let mut parts = vec![format!(
        "{}: net {} for {} is {} (revenue {}, expenses {}).",
        entity_name,
        if cur.net_profit >= 0.0 {
            "profit"
        } else {
            "loss"
        },
        metrics.month,
        money(cur.net_profit.abs()),
        money(cur.revenue),
        money(cur.expenses)
    )];

    if let Some(change) = pct_change(cur.net_profit, prev.net_profit) {
        let direction = if change >= 0.0 { "up" } else { "down" };
        parts.push(format!(
            "That is {} {:.0}% versus last month.",
            direction,
            change.abs()
        ));
    }
    parts.push(format!("Cash and bank position: {}.", money(metrics.cash)));
    parts.push(format!(
        "Open period {}, {} day(s) left in the month.",
        metrics.open_period, metrics.days_left_in_month
    ));
    if alerts.is_empty() {
        parts.push("No issues detected.".to_string());
    }
    parts.join(" ")
}

pub fn narrate_board(metrics: &KanbanBoardMetrics, alerts: &[Alert]) -> String {
    let mut parts = vec![format!(
        "Board {}: {} open task(s), {} completed in the last 7 days.",
        metrics.board, metrics.open_total, metrics.completed_week
    )];
    if metrics.overdue > 0 {
        parts.push(format!("{} overdue.", metrics.overdue));
    }
    if metrics.due_soon > 0 {
        parts.push(format!("{} due soon.", metrics.due_soon));
    }
    if let Some(first) = metrics.top_tasks.first() {
        parts.push(format!(
            "Focus today on '{}' ({}).",
            first.title, first.priority
        ));
    }
    if alerts.is_empty() {
        parts.push("No issues detected.".to_string());
    }
    parts.join(" ")
}

pub fn narrate_leads(entity_name: &str, metrics: &LeadsMetrics, alerts: &[Alert]) -> String {
    let pipeline = money(metrics.pipeline_value);
    let mut parts = vec![format!(
        "{}: {} open lead(s) with pipeline potential of {}.",
        entity_name, metrics.open_total, pipeline
    )];
    if metrics.new_month > 0 {
        parts.push(format!("{} new lead(s) this month.", metrics.new_month));
    }
    if metrics.won_month > 0 {
        parts.push(format!("{} deal(s) won this month.", metrics.won_month));
    }
    if metrics.overdue > 0 {
        parts.push(format!("{} follow-up(s) overdue.", metrics.overdue));
    }
    if metrics.due_today > 0 {
        parts.push(format!("{} due for follow-up today.", metrics.due_today));
    }
    if let Some(first) = metrics.top_leads.first() {
        let co = if !first.company.is_empty() {
            format!(" ({})", first.company)
        } else {
            String::new()
        };
        parts.push(format!(
            "Focus today on '{}'{} [{}].",
            first.name, co, first.priority_title
        ));
    }
    if alerts.is_empty() {
        parts.push("Pipeline is on track.".to_string());
    }
    parts.join(" ")
}

