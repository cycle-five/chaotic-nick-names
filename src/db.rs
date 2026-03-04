use std::collections::{HashMap, HashSet, VecDeque};

use poise::serenity_prelude::GuildId;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

use crate::state::{GuildState, GuildStats, HistoryEntry};

// ── Pool setup ────────────────────────────────────────────────────────────────

/// Connect to Postgres, run pending migrations, and return the pool.
pub async fn setup(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::raw_sql(include_str!("../migrations/001_initial.sql"))
        .execute(&pool)
        .await?;

    Ok(pool)
}

// ── Startup load ─────────────────────────────────────────────────────────────

/// Load all guild states that exist in the database.
pub async fn load_all_guilds(
    pool: &PgPool,
) -> Result<Vec<(GuildId, GuildState)>, sqlx::Error> {
    let guild_ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT guild_id FROM (
            SELECT guild_id FROM guild_stats
            UNION
            SELECT guild_id FROM custom_categories
            UNION
            SELECT guild_id FROM used_names
            UNION
            SELECT guild_id FROM nick_changes
        ) AS g
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for raw_id in guild_ids {
        let guild_id = GuildId::new(raw_id as u64);
        let gs = load_guild(pool, guild_id).await?;
        result.push((guild_id, gs));
    }
    Ok(result)
}

async fn load_guild(pool: &PgPool, guild_id: GuildId) -> Result<GuildState, sqlx::Error> {
    let gid = guild_id.get() as i64;

    // Custom categories
    let cat_rows = sqlx::query(
        "SELECT name, items FROM custom_categories WHERE guild_id = $1",
    )
    .bind(gid)
    .fetch_all(pool)
    .await?;

    let mut custom_categories: HashMap<String, Vec<String>> = HashMap::new();
    for row in cat_rows {
        let name: String = row.get("name");
        let items: Vec<String> = row.get("items");
        custom_categories.insert(name, items);
    }

    // Used names
    let used_rows = sqlx::query(
        "SELECT category_name, name FROM used_names WHERE guild_id = $1",
    )
    .bind(gid)
    .fetch_all(pool)
    .await?;

    let mut used_names: HashMap<String, HashSet<String>> = HashMap::new();
    for row in used_rows {
        let cat: String = row.get("category_name");
        let name: String = row.get("name");
        used_names.entry(cat).or_default().insert(name);
    }

    // History (latest 200)
    let hist_rows = sqlx::query(
        r#"
        SELECT user_id, user_name, old_nick, new_nick, category, changed_at
        FROM nick_changes
        WHERE guild_id = $1
        ORDER BY changed_at DESC
        LIMIT 200
        "#,
    )
    .bind(gid)
    .fetch_all(pool)
    .await?;

    let history: VecDeque<HistoryEntry> = hist_rows
        .into_iter()
        .map(|r| {
            let ts: chrono::DateTime<chrono::Utc> = r.get("changed_at");
            HistoryEntry {
                timestamp: ts,
                user_id: r.get::<i64, _>("user_id") as u64,
                user_name: r.get("user_name"),
                old_nick: r.get("old_nick"),
                new_nick: r.get("new_nick"),
                category: r.get("category"),
            }
        })
        .collect();

    // Stats
    let stats_row = sqlx::query(
        "SELECT total_changes, bulk_randomize_count FROM guild_stats WHERE guild_id = $1",
    )
    .bind(gid)
    .fetch_optional(pool)
    .await?;

    let (total_changes, bulk_randomize_count) = match stats_row {
        Some(r) => (
            r.get::<i64, _>("total_changes") as u64,
            r.get::<i64, _>("bulk_randomize_count") as u64,
        ),
        None => (0, 0),
    };

    // Category usage
    let usage_rows = sqlx::query(
        "SELECT category_name, usage_count FROM category_usage WHERE guild_id = $1",
    )
    .bind(gid)
    .fetch_all(pool)
    .await?;

    let mut category_usage: HashMap<String, u64> = HashMap::new();
    for row in usage_rows {
        let cat: String = row.get("category_name");
        let count: i64 = row.get("usage_count");
        category_usage.insert(cat, count as u64);
    }

    Ok(GuildState {
        custom_categories,
        used_names,
        history,
        stats: GuildStats {
            total_changes,
            bulk_randomize_count,
            category_usage,
        },
    })
}

// ── Write-through helpers ─────────────────────────────────────────────────────

/// Persist (insert or replace) a custom category.
pub async fn upsert_custom_category(
    pool: &PgPool,
    guild_id: GuildId,
    name: &str,
    items: &[String],
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    sqlx::query(
        r#"
        INSERT INTO custom_categories (guild_id, name, items)
        VALUES ($1, $2, $3)
        ON CONFLICT (guild_id, name) DO UPDATE SET items = EXCLUDED.items
        "#,
    )
    .bind(gid)
    .bind(name)
    .bind(items)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a custom category row.
pub async fn delete_custom_category(
    pool: &PgPool,
    guild_id: GuildId,
    name: &str,
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    sqlx::query(
        "DELETE FROM custom_categories WHERE guild_id = $1 AND name = $2",
    )
    .bind(gid)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a single name as used in the without-replacement pool.
pub async fn add_used_name(
    pool: &PgPool,
    guild_id: GuildId,
    category: &str,
    name: &str,
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    sqlx::query(
        r#"
        INSERT INTO used_names (guild_id, category_name, name)
        VALUES ($1, $2, $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(gid)
    .bind(category)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Clear used-name pool for one category, or all categories if `category` is `None`.
pub async fn clear_used_names(
    pool: &PgPool,
    guild_id: GuildId,
    category: Option<&str>,
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    match category {
        Some(cat) => {
            sqlx::query(
                "DELETE FROM used_names WHERE guild_id = $1 AND category_name = $2",
            )
            .bind(gid)
            .bind(cat)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM used_names WHERE guild_id = $1")
                .bind(gid)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Insert a single nick-change row.
pub async fn insert_nick_change(
    pool: &PgPool,
    guild_id: GuildId,
    user_id: u64,
    user_name: &str,
    old_nick: Option<&str>,
    new_nick: &str,
    category: &str,
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    let uid = user_id as i64;
    sqlx::query(
        r#"
        INSERT INTO nick_changes (guild_id, user_id, user_name, old_nick, new_nick, category)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(gid)
    .bind(uid)
    .bind(user_name)
    .bind(old_nick)
    .bind(new_nick)
    .bind(category)
    .execute(pool)
    .await?;
    Ok(())
}

/// Upsert the aggregate stats row for a guild.
pub async fn upsert_guild_stats(
    pool: &PgPool,
    guild_id: GuildId,
    total_changes: u64,
    bulk_randomize_count: u64,
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    sqlx::query(
        r#"
        INSERT INTO guild_stats (guild_id, total_changes, bulk_randomize_count)
        VALUES ($1, $2, $3)
        ON CONFLICT (guild_id) DO UPDATE
            SET total_changes        = EXCLUDED.total_changes,
                bulk_randomize_count = EXCLUDED.bulk_randomize_count
        "#,
    )
    .bind(gid)
    .bind(total_changes as i64)
    .bind(bulk_randomize_count as i64)
    .execute(pool)
    .await?;
    Ok(())
}

/// Increment (or insert) the usage counter for a category in this guild.
pub async fn increment_category_usage(
    pool: &PgPool,
    guild_id: GuildId,
    category: &str,
) -> Result<(), sqlx::Error> {
    let gid = guild_id.get() as i64;
    sqlx::query(
        r#"
        INSERT INTO category_usage (guild_id, category_name, usage_count)
        VALUES ($1, $2, 1)
        ON CONFLICT (guild_id, category_name) DO UPDATE
            SET usage_count = category_usage.usage_count + 1
        "#,
    )
    .bind(gid)
    .bind(category)
    .execute(pool)
    .await?;
    Ok(())
}
