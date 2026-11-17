use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};

fn postgres_sql(sql: &str) -> &'static str {
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let original = sql;
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut entries = cache.lock().expect("SQL cache lock poisoned");
    if let Some(cached) = entries.get(original) {
        return cached;
    }

    let mut sql = sql.to_owned();
    let bool_columns = [
        "is_superuser",
        "is_staff",
        "is_active",
        "is_posted",
        "is_closed",
        "is_completed",
        "is_done",
        "completed",
        "public",
        "active",
        "locked",
        "terkirim_telegram",
    ];
    for column in bool_columns {
        for value in [0, 1] {
            let from = format!("{column} = {value}");
            let to = format!("{column} = {}", if value == 0 { "FALSE" } else { "TRUE" });
            sql = sql.replace(&from, &to);
        }
    }
    // PostgreSQL requires boolean constants in INSERT value lists; the legacy
    // SQL used numeric 0/1 literals for these columns.
    if let Some(insert_at) = sql.to_ascii_uppercase().find("INSERT INTO ") {
        if let Some(cols_open_rel) = sql[insert_at..].find('(') {
            let cols_open = insert_at + cols_open_rel;
            if let Some(cols_close_rel) = sql[cols_open..].find(')') {
                let cols_close = cols_open + cols_close_rel;
                if let Some(values_rel) = sql[cols_close..].to_ascii_uppercase().find("VALUES") {
                    let values_at = cols_close + values_rel + 6;
                    if let Some(vals_open_rel) = sql[values_at..].find('(') {
                        let vals_open = values_at + vals_open_rel;
                        if let Some(vals_close_rel) = sql[vals_open..].find(')') {
                            let vals_close = vals_open + vals_close_rel;
                            let cols: Vec<_> = sql[cols_open + 1..cols_close]
                                .split(',')
                                .map(|v| v.trim())
                                .collect();
                            let mut values: Vec<String> = sql[vals_open + 1..vals_close]
                                .split(',')
                                .map(|v| v.trim().to_owned())
                                .collect();
                            for (index, col) in cols.iter().enumerate() {
                                if bool_columns.contains(col) && index < values.len() {
                                    values[index] = match values[index].as_str() {
                                        "0" => "FALSE".into(),
                                        "1" => "TRUE".into(),
                                        _ => values[index].clone(),
                                    };
                                }
                            }
                            sql.replace_range(vals_open + 1..vals_close, &values.join(", "));
                        }
                    }
                }
            }
        }
    }
    let mut converted = String::with_capacity(sql.len());
    let mut parameter = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    for ch in sql.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                converted.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                converted.push(ch);
            }
            '?' if !in_single && !in_double => {
                parameter += 1;
                converted.push_str(&format!("${parameter}"));
            }
            _ => converted.push(ch),
        }
    }
    let sql: &'static str = Box::leak(converted.into_boxed_str());
    entries.insert(original.to_owned(), sql);
    sql
}

pub fn pg_query<'q>(
    sql: &str,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(postgres_sql(sql))
}

pub fn pg_query_scalar<'q, O>(
    sql: &str,
) -> sqlx::query::QueryScalar<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>
where
    O: sqlx::Type<sqlx::Postgres> + for<'r> sqlx::Decode<'r, sqlx::Postgres>,
{
    sqlx::query_scalar(postgres_sql(sql))
}

pub fn pg_query_as<'q, O>(
    sql: &str,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, O, sqlx::postgres::PgArguments>
where
    O: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow>,
{
    sqlx::query_as(postgres_sql(sql))
}

use crate::auth::hash_django_password;

const SCHEMA_SQL: &str = include_str!("../migrations/001_schema.sql");

pub async fn connect_and_migrate(database_url: &str) -> Result<PgPool> {
    let mut attempts = 0u32;
    let pool = loop {
        attempts += 1;
        match PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(5))
            .connect(database_url)
            .await
        {
            Ok(p) => break p,
            Err(err) => {
                if attempts >= 30 {
                    return Err(err.into());
                }
                tracing::warn!("Waiting for database (attempt {}/30): {}", attempts, err);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };

    run_schema(&pool).await?;
    seed_defaults(&pool).await?;
    Ok(pool)
}

async fn run_schema(pool: &PgPool) -> Result<()> {
    // Strip `--` comments and execute each `;`-terminated statement.
    let mut cleaned = String::with_capacity(SCHEMA_SQL.len());
    for line in SCHEMA_SQL.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("--") || trimmed.is_empty() {
            continue;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }

    for stmt in cleaned.split(';') {
        let sql = stmt.trim();
        if sql.is_empty() {
            continue;
        }
        crate::db::pg_query(sql).execute(pool).await?;
    }
    Ok(())
}

async fn seed_defaults(pool: &PgPool) -> Result<()> {
    let user_count: i64 = crate::db::pg_query("SELECT COUNT(*) FROM auth_user")
        .fetch_one(pool)
        .await?
        .get(0);

    if user_count == 0 {
        let hashed = hash_django_password("admin123");
        crate::db::pg_query(
            "INSERT INTO auth_user (password, is_superuser, username, first_name, last_name, email, is_staff, is_active)
             VALUES (?, 1, 'admin', 'Admin', 'Teknusa', 'admin@teknusa.com', 1, 1)",
        )
        .bind(hashed)
        .execute(pool)
        .await?;
        tracing::info!("Seeded default admin user (admin / admin123)");
    }

    let dc_count: i64 = crate::db::pg_query("SELECT COUNT(*) FROM post_media_digitalchannel")
        .fetch_one(pool)
        .await?
        .get(0);
    if dc_count == 0 {
        for name in [
            "ig",
            "threads",
            "linkedin",
            "linkedin_company",
            "linkedin_newsletter",
            "tiktok",
            "blog",
        ] {
            let _ = crate::db::pg_query(
                "INSERT INTO post_media_digitalchannel (name, description) VALUES (?, '') ON CONFLICT DO NOTHING",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let fs_count: i64 = crate::db::pg_query("SELECT COUNT(*) FROM post_media_funnelstage")
        .fetch_one(pool)
        .await?
        .get(0);
    if fs_count == 0 {
        for name in ["Awareness", "Interest", "Desire", "Action", "Retention"] {
            let _ = crate::db::pg_query(
                "INSERT INTO post_media_funnelstage (name, description) VALUES (?, '') ON CONFLICT DO NOTHING",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let biaya_count: i64 = crate::db::pg_query("SELECT COUNT(*) FROM post_media_biaya")
        .fetch_one(pool)
        .await?
        .get(0);
    if biaya_count == 0 {
        for name in ["free", "paid"] {
            let _ = crate::db::pg_query(
                "INSERT INTO post_media_biaya (name, description) VALUES (?, '') ON CONFLICT DO NOTHING",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let akun_count: i64 = crate::db::pg_query("SELECT COUNT(*) FROM post_media_akun")
        .fetch_one(pool)
        .await?
        .get(0);
    if akun_count == 0 {
        for name in ["pribadi", "teknusa", "Nusa Sawit", "SDMPortabel"] {
            let _ = crate::db::pg_query(
                "INSERT INTO post_media_akun (name, description) VALUES (?, '') ON CONFLICT DO NOTHING",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let market_count: i64 = crate::db::pg_query("SELECT COUNT(*) FROM post_media_market")
        .fetch_one(pool)
        .await?
        .get(0);
    if market_count == 0 {
        for name in ["Teknusa", "Nusa Sawit", "SDMPortabel", "Personal"] {
            let _ = crate::db::pg_query(
                "INSERT INTO post_media_market (name, description) VALUES (?, '') ON CONFLICT DO NOTHING",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    Ok(())
}
