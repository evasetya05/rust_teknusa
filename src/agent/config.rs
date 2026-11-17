#[derive(Clone, Debug)]
pub struct AgentThresholds {
    // Finance
    pub closing_warning_days: i64,
    pub profit_drop_percent: f64,
    pub expense_jump_percent: f64,
    pub expense_jump_min_amount: f64,
    pub max_expense_alerts: usize,
    // Kanban
    pub due_soon_days: i64,
    pub stalled_days: i64,
    pub overdue_critical_count: usize,
    pub top_priority_count: usize,
    // Leads / CRM
    pub lead_due_soon_days: i64,
    pub lead_stalled_days: i64,
    pub lead_overdue_critical_count: usize,
    pub lead_top_priority_count: usize,
}

impl Default for AgentThresholds {
    fn default() -> Self {
        Self {
            closing_warning_days: 5,
            profit_drop_percent: 20.0,
            expense_jump_percent: 30.0,
            expense_jump_min_amount: 1_000_000.0,
            max_expense_alerts: 3,
            due_soon_days: 3,
            stalled_days: 7,
            overdue_critical_count: 5,
            top_priority_count: 3,
            lead_due_soon_days: 3,
            lead_stalled_days: 7,
            lead_overdue_critical_count: 3,
            lead_top_priority_count: 3,
        }
    }
}

