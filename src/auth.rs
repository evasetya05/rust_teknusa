use axum::async_trait;
use axum::extract::FromRequestParts;
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64URL};
use base64::Engine;
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use rand::distributions::Alphanumeric;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tera::Context;

use crate::models::{
    normalize_uuid, Entity, EntityView, FlashMessage, SidebarGroup, SidebarItem, User, UserView,
};
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

pub const SESSION_COOKIE_NAME: &str = "teknusa_session";
const DEFAULT_PBKDF2_ITERATIONS: u32 = 870_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionPayload {
    pub user_id: Option<i32>,
    pub current_entity_id: String,
    #[serde(default)]
    pub flashes: Vec<FlashMessage>,
}

impl Default for SessionPayload {
    fn default() -> Self {
        Self {
            user_id: None,
            current_entity_id: "all".to_string(),
            flashes: Vec::new(),
        }
    }
}

pub fn hash_django_password(password: &str) -> String {
    let salt: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(22)
        .map(char::from)
        .collect();
    hash_django_password_with_salt(password, &salt, DEFAULT_PBKDF2_ITERATIONS)
}

pub fn hash_django_password_with_salt(password: &str, salt: &str, iterations: u32) -> String {
    let mut dk = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iterations, &mut dk);
    let encoded_hash = B64.encode(dk);
    format!("pbkdf2_sha256${}${}${}", iterations, salt, encoded_hash)
}

pub fn verify_django_password(password: &str, encoded: &str) -> bool {
    let parts: Vec<&str> = encoded.split('$').collect();
    if parts.len() != 4 || parts[0] != "pbkdf2_sha256" {
        return false;
    }
    let iterations: u32 = match parts[1].parse() {
        Ok(v) if v > 0 => v,
        _ => return false,
    };
    let salt = parts[2];
    let expected_bytes = match B64.decode(parts[3]) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut dk = vec![0u8; expected_bytes.len()];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), iterations, &mut dk);
    constant_time_eq(&dk, &expected_bytes)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn sign_data(secret: &str, payload_b64: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload_b64.as_bytes());
    B64URL.encode(mac.finalize().into_bytes())
}

pub fn encode_session_cookie(secret: &str, payload: &SessionPayload) -> String {
    let json = serde_json::to_vec(payload).unwrap_or_else(|_| b"{}".to_vec());
    let payload_b64 = B64URL.encode(json);
    let sig = sign_data(secret, &payload_b64);
    format!("{}.{}", payload_b64, sig)
}

pub fn decode_session_cookie(secret: &str, raw: &str) -> Option<SessionPayload> {
    let (payload_b64, sig_b64) = raw.split_once('.')?;
    let expected_sig = sign_data(secret, payload_b64);
    if !constant_time_eq(sig_b64.as_bytes(), expected_sig.as_bytes()) {
        return None;
    }
    let json_bytes = B64URL.decode(payload_b64).ok()?;
    serde_json::from_slice::<SessionPayload>(&json_bytes).ok()
}

pub fn build_session_cookie_header(secret: &str, payload: &SessionPayload) -> HeaderValue {
    let val = encode_session_cookie(secret, payload);
    let cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=1209600",
        SESSION_COOKIE_NAME, val
    );
    HeaderValue::from_str(&cookie)
        .unwrap_or_else(|_| HeaderValue::from_static("teknusa_session=; Path=/"))
}

pub fn clear_session_cookie_header() -> HeaderValue {
    HeaderValue::from_static("teknusa_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

fn extract_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let trimmed = part.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

#[derive(Clone, Debug)]
pub struct SessionContext {
    pub payload: SessionPayload,
    pub user: Option<User>,
    pub user_view: Option<UserView>,
    pub entities: Vec<EntityView>,
    pub current_entity: Option<EntityView>,
    pub current_entity_id: String,
    pub sidebar_menu: Vec<SidebarGroup>,
    pub current_path: String,
    pub flashes: Vec<FlashMessage>,
}

impl SessionContext {
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    pub fn is_admin(&self) -> bool {
        self.user.as_ref().map(|u| u.is_admin()).unwrap_or(false)
    }

    pub fn entity_filter_id(&self) -> Option<String> {
        self.current_entity.as_ref().map(|e| e.id.clone())
    }

    pub fn require_auth_redirect(&self) -> Result<&User, Response> {
        match &self.user {
            Some(u) => Ok(u),
            None => {
                let target = format!("/login/?next={}", urlencoding::encode(&self.current_path));
                Err(Redirect::to(&target).into_response())
            }
        }
    }

    pub fn require_admin_or_403(&self) -> Result<&User, Response> {
        let user = self.require_auth_redirect()?;
        if !user.is_admin() {
            return Err((
                StatusCode::FORBIDDEN,
                Html("<h1>403 Forbidden</h1><p>Akses terbatas hanya untuk administrator.</p>"),
            )
                .into_response());
        }
        Ok(user)
    }

    pub fn push_flash(&mut self, level: &str, text: impl Into<String>) {
        self.payload.flashes.push(FlashMessage {
            level: level.to_string(),
            text: text.into(),
        });
    }

    pub fn apply_to_tera(&self, ctx: &mut Context) {
        if let Some(uv) = &self.user_view {
            ctx.insert("user", uv);
        } else {
            ctx.insert(
                "user",
                &serde_json::json!({
                    "is_authenticated": false,
                    "is_admin": false,
                    "username": "",
                    "full_name": "",
                }),
            );
        }
        ctx.insert("entities", &self.entities);
        if let Some(ce) = &self.current_entity {
            ctx.insert("current_entity", ce);
        } else {
            ctx.insert("current_entity", &Option::<EntityView>::None);
        }
        ctx.insert("current_entity_id", &self.current_entity_id);
        ctx.insert("sidebar_menu", &self.sidebar_menu);
        ctx.insert("messages", &self.flashes);
        ctx.insert("current_path", &self.current_path);
    }
}

#[async_trait]
impl FromRequestParts<AppState> for SessionContext {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current_path = parts.uri.path().to_string();
        let mut payload = extract_cookie_value(&parts.headers, SESSION_COOKIE_NAME)
            .and_then(|raw| decode_session_cookie(&state.config.secret_key, &raw))
            .unwrap_or_default();

        let user = if let Some(uid) = payload.user_id {
            sqlx::query_as::<_, User>(
                "SELECT id, password, last_login, is_superuser, username, first_name, last_name, email, is_staff, is_active, date_joined
                 FROM auth_user WHERE id = ? AND is_active = 1 LIMIT 1",
            )
            .bind(uid)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
        } else {
            None
        };

        if user.is_none() {
            payload.user_id = None;
        }

        let user_view = user.as_ref().map(UserView::from);

        let raw_entities = if user.is_some() {
            sqlx::query_as::<_, Entity>(
                "SELECT id, parent_id, name, code, description, email, phone, logo, is_active, created_at, updated_at
                 FROM entity_entity WHERE is_active = 1 ORDER BY name ASC",
            )
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
        } else {
            Vec::new()
        };

        let entities: Vec<EntityView> = raw_entities
            .iter()
            .map(|e| EntityView::from_entity(e, None))
            .collect();

        let mut current_entity_id = payload.current_entity_id.clone();
        if current_entity_id.is_empty() {
            current_entity_id = "all".to_string();
        }
        let mut current_entity = None;
        if current_entity_id != "all" {
            let norm = normalize_uuid(&current_entity_id);
            current_entity = entities
                .iter()
                .find(|e| normalize_uuid(&e.id) == norm)
                .cloned();
            if current_entity.is_none() {
                current_entity_id = "all".to_string();
            } else {
                current_entity_id = current_entity.as_ref().unwrap().id.clone();
            }
        }
        payload.current_entity_id = current_entity_id.clone();

        let flashes = payload.flashes.clone();
        let sidebar_menu = build_sidebar_menu(&current_path);

        Ok(SessionContext {
            payload,
            user,
            user_view,
            entities,
            current_entity,
            current_entity_id,
            sidebar_menu,
            current_path,
            flashes,
        })
    }
}

pub fn build_sidebar_menu(path: &str) -> Vec<SidebarGroup> {
    vec![
        SidebarGroup {
            section: "Perusahaan Aktif".to_string(),
            items: vec![SidebarItem {
                title: "Kelola Perusahaan".to_string(),
                icon: "fas fa-building".to_string(),
                url: "/entity/".to_string(),
                active: path.starts_with("/entity"),
                indent: false,
                class: "text-primary fw-bold".to_string(),
            }],
        },
        SidebarGroup {
            section: "Aplikasi".to_string(),
            items: vec![
                SidebarItem {
                    title: "Dashboard".to_string(),
                    icon: "fas fa-tachometer-alt".to_string(),
                    url: "/dashboard/".to_string(),
                    active: path == "/dashboard/" || path == "/dashboard",
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Leads".to_string(),
                    icon: "fas fa-user-plus".to_string(),
                    url: "/leads/".to_string(),
                    active: path.contains("leads") && !path.contains("leads/calendar"),
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Lead Calendar".to_string(),
                    icon: "fas fa-calendar-alt".to_string(),
                    url: "/leads/calendar/".to_string(),
                    active: path.contains("leads/calendar"),
                    indent: true,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Lean".to_string(),
                    icon: "fas fa-stream".to_string(),
                    url: "/lean/".to_string(),
                    active: path.contains("/lean"),
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Ledger".to_string(),
                    icon: "fas fa-book".to_string(),
                    url: "/ledger/".to_string(),
                    active: path.contains("/ledger"),
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Post Media".to_string(),
                    icon: "fas fa-photo-video".to_string(),
                    url: "/post_media/".to_string(),
                    active: path.contains("/post_media"),
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Kanban".to_string(),
                    icon: "fas fa-columns".to_string(),
                    url: "/kanban/".to_string(),
                    active: path.contains("/kanban") && !path.contains("calendar"),
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Kanban Calendar".to_string(),
                    icon: "fas fa-calendar-alt".to_string(),
                    url: "/kanban/calendar/".to_string(),
                    active: path.contains("/kanban") && path.contains("calendar"),
                    indent: true,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Gantt Chart".to_string(),
                    icon: "fas fa-stream".to_string(),
                    url: "/gantt/".to_string(),
                    active: path.starts_with("/gantt"),
                    indent: true,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Agent".to_string(),
                    icon: "fas fa-robot".to_string(),
                    url: "/agent/briefing/".to_string(),
                    active: path.contains("/agent") && !path.contains("/llm"),
                    indent: false,
                    class: String::new(),
                },
                SidebarItem {
                    title: "Test LLM".to_string(),
                    icon: "fas fa-vial".to_string(),
                    url: "/agent/llm/".to_string(),
                    active: path.contains("agent/llm"),
                    indent: true,
                    class: String::new(),
                },
            ],
        },
        SidebarGroup {
            section: String::new(),
            items: vec![SidebarItem {
                title: "Logout".to_string(),
                icon: "fas fa-sign-out-alt".to_string(),
                url: "/logout/".to_string(),
                active: false,
                indent: false,
                class: "text-danger".to_string(),
            }],
        },
    ]
}

pub fn render_page(
    state: &AppState,
    session: &SessionContext,
    template_name: &str,
    mut ctx: Context,
) -> Response {
    session.apply_to_tera(&mut ctx);

    let html = match state.tera.render(template_name, &ctx) {
        Ok(rendered) => rendered,
        Err(err) => {
            tracing::error!("Template render error ({}): {:?}", template_name, err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!(
                    "<h1>Template Render Error</h1><pre>{}</pre>",
                    err
                )),
            )
                .into_response();
        }
    };

    if session.flashes.is_empty() {
        Html(html).into_response()
    } else {
        let mut clean_payload = session.payload.clone();
        clean_payload.flashes.clear();
        let mut headers = HeaderMap::new();
        headers.insert(
            SET_COOKIE,
            build_session_cookie_header(&state.config.secret_key, &clean_payload),
        );
        (headers, Html(html)).into_response()
    }
}

pub fn redirect_with_session(state: &AppState, payload: &SessionPayload, to: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        build_session_cookie_header(&state.config.secret_key, payload),
    );
    (headers, Redirect::to(to)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_django_pbkdf2_hash_and_verify() {
        let encoded = hash_django_password("SecretPass123!");
        assert!(encoded.starts_with("pbkdf2_sha256$870000$"));
        assert!(verify_django_password("SecretPass123!", &encoded));
        assert!(!verify_django_password("WrongPass", &encoded));
    }

    #[test]
    fn test_django_known_format_verify() {
        let encoded = hash_django_password_with_salt("admin123", "testSalt1234", 1000);
        assert!(verify_django_password("admin123", &encoded));
        assert!(!verify_django_password("admin124", &encoded));
    }

    #[test]
    fn test_session_cookie_roundtrip_and_tamper() {
        let secret = "test-secret-key";
        let payload = SessionPayload {
            user_id: Some(42),
            current_entity_id: "abc123".to_string(),
            flashes: vec![FlashMessage {
                level: "success".to_string(),
                text: "Saved!".to_string(),
            }],
        };
        let encoded = encode_session_cookie(secret, &payload);
        let decoded = decode_session_cookie(secret, &encoded).expect("should decode");
        assert_eq!(decoded.user_id, Some(42));
        assert_eq!(decoded.current_entity_id, "abc123");
        assert_eq!(decoded.flashes.len(), 1);
        assert!(decode_session_cookie("wrong-secret", &encoded).is_none());
    }
}

