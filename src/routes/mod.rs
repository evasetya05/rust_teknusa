pub mod agent;
pub mod auth;
pub mod dashboard;
pub mod entity;
pub mod gantt;
pub mod kanban;
pub mod leads;
pub mod lean;
pub mod ledger;
pub mod post_media;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn build_router() -> Router<AppState> {
    Router::new()
        // Root & Auth
        .route("/", get(auth::root_redirect))
        .route("/login/", get(auth::login_get).post(auth::login_post))
        .route("/register/", get(auth::register_get).post(auth::register_post))
        .route("/logout/", get(auth::logout_handler).post(auth::logout_handler))
        .route("/account/result.html", get(auth::account_result))
        // Dashboard
        .route("/dashboard/", get(dashboard::dashboard_view))
        // Entity
        .route("/entity/", get(entity::entity_list))
        .route(
            "/entity/create/",
            get(entity::entity_create_get).post(entity::entity_create_post),
        )
        .route(
            "/entity/:pk/edit/",
            get(entity::entity_edit_get).post(entity::entity_edit_post),
        )
        .route(
            "/entity/:pk/delete/",
            get(entity::entity_delete_get).post(entity::entity_delete_post),
        )
        .route("/entity/select/", post(entity::select_entity))
        // Leads
        .route("/leads/", get(leads::lead_list))
        .route("/leads/lead/:pk/", get(leads::lead_detail_get).post(leads::lead_detail_post))
        .route("/leads/create/", get(leads::lead_create_get).post(leads::lead_create_post))
        .route(
            "/leads/lead/:pk/edit/",
            get(leads::lead_edit_get).post(leads::lead_edit_post),
        )
        .route("/leads/analysis/", get(leads::lead_analysis))
        .route("/leads/graphic/", get(leads::lead_graphic))
        .route("/leads/calendar/", get(leads::lead_calendar))
        .route("/leads/calendar/events/", get(leads::lead_calendar_events))
        // Lean
        .route("/lean/", get(lean::idea_list))
        .route("/lean/ideas/create/", get(lean::idea_create_get).post(lean::idea_create_post))
        .route("/lean/idea/:pk/", get(lean::idea_detail))
        .route("/lean/idea/:pk/edit/", get(lean::idea_edit_get).post(lean::idea_edit_post))
        .route("/lean/idea/:pk/build/add/", post(lean::build_add_post))
        .route("/lean/build/:pk/edit/", get(lean::build_edit_get).post(lean::build_edit_post))
        .route("/lean/build/:pk/product/add/", post(lean::product_add_post))
        .route("/lean/product/:pk/edit/", get(lean::product_edit_get).post(lean::product_edit_post))
        .route("/lean/product/:pk/measure/add/", post(lean::measure_add_post))
        .route("/lean/measure/:pk/edit/", get(lean::measure_edit_get).post(lean::measure_edit_post))
        .route("/lean/measure/:pk/datapoint/add/", post(lean::datapoint_add_post))
        .route(
            "/lean/datapoint/:pk/edit/",
            get(lean::datapoint_edit_get).post(lean::datapoint_edit_post),
        )
        .route("/lean/idea/:pk/learning/add/", post(lean::learning_add_post))
        .route(
            "/lean/learning/:pk/edit/",
            get(lean::learning_edit_get).post(lean::learning_edit_post),
        )
        // Ledger
        .route("/ledger/", get(ledger::ledger_index))
        .route("/ledger/accounts/accounts/", get(ledger::account_list))
        .route(
            "/ledger/accounts/accounts/new/",
            get(ledger::account_create_get).post(ledger::account_create_post),
        )
        .route(
            "/ledger/journal/journal/create/",
            get(ledger::create_journal_entry_get).post(ledger::create_journal_entry_post),
        )
        .route("/ledger/journal/journal/list/", get(ledger::journal_list))
        .route(
            "/ledger/journal/journal/edit/:pk/",
            get(ledger::journal_edit_get).post(ledger::journal_edit_post),
        )
        .route("/ledger/closing/closing-periods/", get(ledger::closing_period_list))
        .route(
            "/ledger/closing/closing-periods/:period/close/",
            get(ledger::close_period).post(ledger::close_period),
        )
        .route("/ledger/report/report/", get(ledger::ledger_report))
        .route(
            "/ledger/report/report/profit-loss/",
            get(ledger::profit_and_loss_report),
        )
        .route(
            "/ledger/accounts/ledger/report/balance-sheet/",
            get(ledger::balance_sheet_view),
        )
        .route("/ledger/profitabilitas/profitabilitas/", get(ledger::profitabilitas_view))
        .route("/ledger/solvabilitas/solvabilitas/", get(ledger::solvabilitas_view))
        .route("/ledger/report/dimension-analysis/", get(ledger::dimension_report))
        .route(
            "/ledger/report/report/dimension-analysis/",
            get(ledger::dimension_report),
        )
        .route("/ledger/dimensions/", get(ledger::dimensions_manage_get))
        .route("/ledger/dimensions/create/", post(ledger::dimensions_create_post))
        // Post Media
        .route("/post_media/", get(post_media::channel_list))
        .route("/post_media/post_media_home/", get(post_media::post_media_home))
        .route("/post_media/channel/:pk/", get(post_media::channel_detail))
        .route(
            "/post_media/add/",
            get(post_media::channel_add_get).post(post_media::channel_add_post),
        )
        .route(
            "/post_media/edit/:pk/",
            get(post_media::channel_edit_get).post(post_media::channel_edit_post),
        )
        .route("/post_media/analytics/", get(post_media::channel_analytics))
        .route(
            "/post_media/analytics/export/",
            get(post_media::export_channel_analytics),
        )
        .route(
            "/post_media/channel/:pk/update-performance/",
            get(post_media::update_performance_get).post(post_media::update_performance_post),
        )
        .route("/post_media/ide-konten/", get(post_media::ide_konten_list))
        .route("/post_media/ide-konten/generate/", post(post_media::ide_konten_generate))
        // Kanban
        .route("/kanban/", get(kanban::board_list))
        .route("/kanban/calendar/", get(kanban::kanban_calendar))
        .route("/kanban/board/:pk/", get(kanban::board_detail))
        .route("/kanban/board/:pk/calendar/", get(kanban::board_calendar))
        .route(
            "/kanban/board/add/",
            get(kanban::board_add_get).post(kanban::board_add_post),
        )
        .route(
            "/kanban/task/add/",
            get(kanban::task_add_get).post(kanban::task_add_post),
        )
        .route(
            "/kanban/task/:pk/edit/",
            get(kanban::task_edit_get).post(kanban::task_edit_post),
        )
        .route("/kanban/task/:pk/delete/", post(kanban::task_delete_post))
        .route("/kanban/task/move/", post(kanban::task_move_post))
        .route(
            "/kanban/task/:task_id/comment/add/",
            get(kanban::task_comment_add_get).post(kanban::task_comment_add_post),
        )
        .route(
            "/kanban/comment/:pk/delete/",
            post(kanban::task_comment_delete_post),
        )
        .route(
            "/kanban/task/:task_id/attachment/add/",
            get(kanban::task_attachment_add_get).post(kanban::task_attachment_add_post),
        )
        .route(
            "/kanban/attachment/:pk/delete/",
            post(kanban::task_attachment_delete_post),
        )
        .route(
            "/kanban/task/:task_id/checklist/add/",
            post(kanban::task_checklist_add_post),
        )
        .route(
            "/kanban/checklist/:pk/toggle/",
            post(kanban::task_checklist_toggle_post),
        )
        .route(
            "/kanban/checklist/:pk/delete/",
            post(kanban::task_checklist_delete_post),
        )
        // Gantt
        .route("/gantt/", get(gantt::gantt_view))
        .route("/gantt/task/:task_id/dates/", post(gantt::update_task_dates))
        .route("/gantt/dependency/add/", post(gantt::dependency_add))
        .route("/gantt/dependency/:pk/delete/", post(gantt::dependency_delete))
        .route("/gantt/milestone/add/", post(gantt::milestone_add))
        .route("/gantt/milestone/:pk/toggle/", post(gantt::milestone_toggle))
        .route("/gantt/milestone/:pk/move/", post(gantt::milestone_move))
        .route("/gantt/milestone/:pk/delete/", post(gantt::milestone_delete))
        .route("/gantt/baseline/set/", post(gantt::baseline_set))
        .route("/gantt/baseline/clear/", post(gantt::baseline_clear))
        // Agent AI & LLM
        .route("/agent/briefing/", get(agent::briefing_view))
        .route("/agent/llm/", get(agent::llm_status_view))
        .route("/agent/llm/settings/", post(agent::llm_settings_view))
        .route("/agent/llm/test/", post(agent::llm_test_view))
        .route("/agent/llm/test-suite/", post(agent::llm_test_suite_view))
        .route("/agent/chat/", post(agent::chat_view))
}

