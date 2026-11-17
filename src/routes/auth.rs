use axum::extract::{Query, State};
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Form;
use serde::Deserialize;
use tera::Context;

use crate::auth::{
    clear_session_cookie_header, hash_django_password, redirect_with_session, render_page,
    verify_django_password, SessionContext, SessionPayload,
};
use crate::models::{FlashMessage, User};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginQuery {
    pub next: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub next: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub email: String,
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    pub password1: String,
    pub password2: String,
}

#[derive(Deserialize)]
pub struct AccountResultQuery {
    #[serde(rename = "type")]
    pub result_type: Option<String>,
}

pub async fn root_redirect(session: SessionContext) -> Response {
    if session.is_authenticated() {
        Redirect::to("/dashboard/").into_response()
    } else {
        Redirect::to("/login/").into_response()
    }
}

pub async fn login_get(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<LoginQuery>,
) -> Response {
    if session.is_authenticated() {
        let next = sanitize_next(q.next.as_deref());
        return Redirect::to(&next).into_response();
    }
    let mut ctx = Context::new();
    ctx.insert("next", &sanitize_next(q.next.as_deref()));
    render_page(&state, &session, "account/login.html", ctx)
}

pub async fn login_post(
    State(state): State<AppState>,
    session: SessionContext,
    Form(form): Form<LoginForm>,
) -> Response {
    let username = form.username.trim();
    let user = crate::db::pg_query_as::<User>(
        "SELECT id, password, last_login, is_superuser, username, first_name, last_name, email, is_staff, is_active, date_joined
         FROM auth_user WHERE username = ? LIMIT 1",
    )
    .bind(username)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let mut ctx = Context::new();
    ctx.insert("username", username);
    ctx.insert("next", &sanitize_next(form.next.as_deref()));

    match user {
        Some(u) if u.is_active && verify_django_password(&form.password, &u.password) => {
            let _ = crate::db::pg_query("UPDATE auth_user SET last_login = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(u.id)
                .execute(&state.db)
                .await;

            let new_payload = SessionPayload {
                user_id: Some(u.id),
                current_entity_id: session.current_entity_id.clone(),
                flashes: vec![FlashMessage {
                    level: "success".to_string(),
                    text: format!("Selamat datang, {}!", u.display_name()),
                }],
            };
            let dest = sanitize_next(form.next.as_deref());
            redirect_with_session(&state, &new_payload, &dest)
        }
        Some(u) if !u.is_active => {
            ctx.insert("error", "Akun tidak aktif.");
            render_page(&state, &session, "account/login.html", ctx)
        }
        _ => {
            ctx.insert("error", "Username atau password tidak sesuai.");
            render_page(&state, &session, "account/login.html", ctx)
        }
    }
}

pub async fn logout_handler() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, clear_session_cookie_header());
    (headers, Redirect::to("/login/")).into_response()
}

pub async fn register_get(State(state): State<AppState>, session: SessionContext) -> Response {
    if session.is_authenticated() {
        return Redirect::to("/dashboard/").into_response();
    }
    render_page(&state, &session, "account/register.html", Context::new())
}

pub async fn register_post(
    State(state): State<AppState>,
    mut session: SessionContext,
    Form(form): Form<RegisterForm>,
) -> Response {
    let username = form.username.trim();
    let email = form.email.trim();
    let mut ctx = Context::new();
    ctx.insert("username", username);
    ctx.insert("email", email);
    ctx.insert("first_name", form.first_name.trim());
    ctx.insert("last_name", form.last_name.trim());

    if username.is_empty() || form.password1.is_empty() {
        ctx.insert("error", "Username dan password wajib diisi.");
        return render_page(&state, &session, "account/register.html", ctx);
    }
    if form.password1 != form.password2 {
        ctx.insert("error", "Konfirmasi password tidak cocok.");
        return render_page(&state, &session, "account/register.html", ctx);
    }

    let existing: Option<(i32,)> =
        crate::db::pg_query_as("SELECT id FROM auth_user WHERE username = ? LIMIT 1")
            .bind(username)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    if existing.is_some() {
        ctx.insert("error", "Username sudah digunakan.");
        return render_page(&state, &session, "account/register.html", ctx);
    }

    let hashed = hash_django_password(&form.password1);
    let res = crate::db::pg_query(
        "INSERT INTO auth_user (password, is_superuser, username, first_name, last_name, email, is_staff, is_active, date_joined)
         VALUES (?, 0, ?, ?, ?, ?, 0, 1, CURRENT_TIMESTAMP)",
    )
    .bind(hashed)
    .bind(username)
    .bind(form.first_name.trim())
    .bind(form.last_name.trim())
    .bind(email)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => {
            session.push_flash("success", "Registrasi berhasil! Silakan login.");
            redirect_with_session(&state, &session.payload, "/login/")
        }
        Err(e) => {
            ctx.insert("error", &format!("Gagal mendaftar: {}", e));
            render_page(&state, &session, "account/register.html", ctx)
        }
    }
}

pub async fn account_result(
    State(state): State<AppState>,
    session: SessionContext,
    Query(q): Query<AccountResultQuery>,
) -> Response {
    let mut ctx = Context::new();
    let title = match q.result_type.as_deref() {
        Some("register") => "Registration Successful",
        Some("validation") => "Validation Successful",
        _ => "Account Status",
    };
    ctx.insert("title", title);
    ctx.insert(
        "content",
        "Akun Anda sudah aktif dan dapat digunakan untuk login ke Teknusa ERP.",
    );
    render_page(&state, &session, "account/result.html", ctx)
}

fn sanitize_next(raw: Option<&str>) -> String {
    match raw {
        Some(s) if s.starts_with('/') && !s.starts_with("//") => s.to_string(),
        _ => "/dashboard/".to_string(),
    }
}

