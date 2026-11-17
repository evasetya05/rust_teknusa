use std::time::Duration;

use anyhow::Result;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySqlPool, Row};

use crate::auth::hash_django_password;

const SCHEMA_SQL: &str = include_str!("../migrations/001_schema.sql");

pub async fn connect_and_migrate(database_url: &str) -> Result<MySqlPool> {
    let mut attempts = 0u32;
    let pool = loop {
        attempts += 1;
        match MySqlPoolOptions::new()
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
                tracing::warn!(
                    "Waiting for database (attempt {}/30): {}",
                    attempts,
                    err
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };

    run_schema(&pool).await?;
    seed_defaults(&pool).await?;
    Ok(pool)
}

async fn run_schema(pool: &MySqlPool) -> Result<()> {
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
        sqlx::query(sql).execute(pool).await?;
    }
    Ok(())
}

async fn seed_defaults(pool: &MySqlPool) -> Result<()> {
    let user_count: i64 = sqlx::query("SELECT COUNT(*) FROM auth_user")
        .fetch_one(pool)
        .await?
        .get(0);

    if user_count == 0 {
        let hashed = hash_django_password("admin123");
        sqlx::query(
            "INSERT INTO auth_user (password, is_superuser, username, first_name, last_name, email, is_staff, is_active)
             VALUES (?, 1, 'admin', 'Admin', 'Teknusa', 'admin@teknusa.com', 1, 1)",
        )
        .bind(hashed)
        .execute(pool)
        .await?;
        tracing::info!("Seeded default admin user (admin / admin123)");
    }

    let dc_count: i64 = sqlx::query("SELECT COUNT(*) FROM post_media_digitalchannel")
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
            let _ = sqlx::query(
                "INSERT IGNORE INTO post_media_digitalchannel (name, description) VALUES (?, '')",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let fs_count: i64 = sqlx::query("SELECT COUNT(*) FROM post_media_funnelstage")
        .fetch_one(pool)
        .await?
        .get(0);
    if fs_count == 0 {
        for name in ["Awareness", "Interest", "Desire", "Action", "Retention"] {
            let _ = sqlx::query(
                "INSERT IGNORE INTO post_media_funnelstage (name, description) VALUES (?, '')",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let biaya_count: i64 = sqlx::query("SELECT COUNT(*) FROM post_media_biaya")
        .fetch_one(pool)
        .await?
        .get(0);
    if biaya_count == 0 {
        for name in ["free", "paid"] {
            let _ = sqlx::query(
                "INSERT IGNORE INTO post_media_biaya (name, description) VALUES (?, '')",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let akun_count: i64 = sqlx::query("SELECT COUNT(*) FROM post_media_akun")
        .fetch_one(pool)
        .await?
        .get(0);
    if akun_count == 0 {
        for name in ["pribadi", "teknusa", "Nusa Sawit", "SDMPortabel"] {
            let _ = sqlx::query(
                "INSERT IGNORE INTO post_media_akun (name, description) VALUES (?, '')",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    let market_count: i64 = sqlx::query("SELECT COUNT(*) FROM post_media_market")
        .fetch_one(pool)
        .await?
        .get(0);
    if market_count == 0 {
        for name in ["Teknusa", "Nusa Sawit", "SDMPortabel", "Personal"] {
            let _ = sqlx::query(
                "INSERT IGNORE INTO post_media_market (name, description) VALUES (?, '')",
            )
            .bind(name)
            .execute(pool)
            .await;
        }
    }

    Ok(())
}
