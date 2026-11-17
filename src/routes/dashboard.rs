use axum::extract::State;
use axum::response::Response;
use tera::Context;

use crate::agent::collectors::build_briefings;
use crate::auth::{render_page, SessionContext};
use crate::state::AppState;

pub async fn dashboard_view(State(state): State<AppState>, session: SessionContext) -> Response {
    if let Err(resp) = session.require_auth_redirect() {
        return resp;
    }

    let mut ctx = Context::new();
    if session.is_admin() {
        let briefings = build_briefings(&state, "all").await;
        ctx.insert("finance", &briefings.finance);
        ctx.insert("kanban", &briefings.kanban);
        ctx.insert("kanban_by_entity", &briefings.kanban_by_entity);
        ctx.insert("leads", &briefings.leads);
        ctx.insert("show_briefing", &true);
    } else {
        ctx.insert("show_briefing", &false);
    }
    render_page(&state, &session, "dashboard/dashboard.html", ctx)
}

