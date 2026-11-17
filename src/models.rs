use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub password: String,
    pub last_login: Option<NaiveDateTime>,
    pub is_superuser: bool,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub is_staff: bool,
    pub is_active: bool,
    pub date_joined: NaiveDateTime,
}

impl User {
    pub fn display_name(&self) -> String {
        let full = format!("{} {}", self.first_name.trim(), self.last_name.trim())
            .trim()
            .to_string();
        if full.is_empty() {
            self.username.clone()
        } else {
            full
        }
    }

    pub fn is_admin(&self) -> bool {
        self.is_staff || self.is_superuser || self.username == "admin"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserView {
    pub id: i32,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub full_name: String,
    pub email: String,
    pub is_staff: bool,
    pub is_superuser: bool,
    pub is_active: bool,
    pub is_authenticated: bool,
    pub is_admin: bool,
    pub date_joined_fmt: String,
}

impl From<&User> for UserView {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            username: u.username.clone(),
            first_name: u.first_name.clone(),
            last_name: u.last_name.clone(),
            full_name: u.display_name(),
            email: u.email.clone(),
            is_staff: u.is_staff,
            is_superuser: u.is_superuser,
            is_active: u.is_active,
            is_authenticated: true,
            is_admin: u.is_admin(),
            date_joined_fmt: u.date_joined.format("%b %Y").to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Entity {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub code: String,
    pub description: String,
    pub email: String,
    pub phone: String,
    pub logo: Option<String>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityView {
    pub id: String,
    pub parent_id: Option<String>,
    pub parent_name: Option<String>,
    pub name: String,
    pub code: String,
    pub description: String,
    pub email: String,
    pub phone: String,
    pub logo_url: Option<String>,
    pub initials: String,
    pub is_active: bool,
    pub created_at: String,
}

impl EntityView {
    pub fn from_entity(e: &Entity, parent_name: Option<String>) -> Self {
        let initials: String = e.name.chars().take(2).collect::<String>().to_uppercase();
        let logo_url = e.logo.as_ref().and_then(|l| {
            let trimmed = l.trim();
            if trimmed.is_empty() {
                None
            } else if trimmed.starts_with('/') || trimmed.starts_with("http") {
                Some(trimmed.to_string())
            } else {
                Some(format!("/media/{}", trimmed))
            }
        });
        Self {
            id: e.id.clone(),
            parent_id: e.parent_id.clone(),
            parent_name,
            name: e.name.clone(),
            code: e.code.clone(),
            description: e.description.clone(),
            email: e.email.clone(),
            phone: e.phone.clone(),
            logo_url,
            initials,
            is_active: e.is_active,
            created_at: e.created_at.format("%d %b %Y").to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarItem {
    pub title: String,
    pub icon: String,
    pub url: String,
    pub active: bool,
    pub indent: bool,
    pub class: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SidebarGroup {
    pub section: String,
    pub items: Vec<SidebarItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlashMessage {
    pub level: String, // success | error | warning | info
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptionItem {
    pub value: String,
    pub label: String,
}

pub fn normalize_uuid(raw: &str) -> String {
    raw.trim().replace('-', "").to_lowercase()
}

pub fn new_uuid_hex() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

