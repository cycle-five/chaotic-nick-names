use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use poise::serenity_prelude::GuildId;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};

use crate::state::{GuildState, GuildStats, HistoryEntry};

// ── Pool setup ────────────────────────────────────────────────────────────────

/// Connect to Postgres, run pending migrations, and return the pool.
/// Migrations are listed explicitly so the boot order is obvious; each file
/// is idempotent (`IF NOT EXISTS`) so re-running on an up-to-date schema is
/// a no-op.
pub async fn setup(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    for sql in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_feedback.sql"),
    ] {
        sqlx::raw_sql(sql).execute(&pool).await?;
    }

    Ok(pool)
}

// ── Startup load ─────────────────────────────────────────────────────────────

/// Load all guild states that exist in the database.
pub async fn load_all_guilds(pool: &PgPool) -> Result<Vec<(GuildId, GuildState)>, sqlx::Error> {
    let guild_ids: Vec<i64> = sqlx::query_scalar(
        r"
        SELECT DISTINCT guild_id FROM (
            SELECT guild_id FROM guild_stats
            UNION
            SELECT guild_id FROM custom_categories
            UNION
            SELECT guild_id FROM used_names
            UNION
            SELECT guild_id FROM nick_changes
        ) AS g
        ",
    )
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for raw_id in guild_ids {
        let guild_id = GuildId::new(raw_id.cast_unsigned());
        let gs = load_guild(pool, guild_id).await?;
        result.push((guild_id, gs));
    }
    Ok(result)
}

async fn load_guild(pool: &PgPool, guild_id: GuildId) -> Result<GuildState, sqlx::Error> {
    let gid = guild_id.get().cast_signed();

    // Custom categories
    let cat_rows = sqlx::query("SELECT name, items FROM custom_categories WHERE guild_id = $1")
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
    let used_rows = sqlx::query("SELECT category_name, name FROM used_names WHERE guild_id = $1")
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
        r"
        SELECT user_id, user_name, old_nick, new_nick, category, changed_at
        FROM nick_changes
        WHERE guild_id = $1
        ORDER BY changed_at DESC
        LIMIT 200
        ",
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
                user_id: r.get::<i64, _>("user_id").cast_unsigned(),
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
            r.get::<i64, _>("total_changes").cast_unsigned(),
            r.get::<i64, _>("bulk_randomize_count").cast_unsigned(),
        ),
        None => (0, 0),
    };

    // Category usage
    let usage_rows =
        sqlx::query("SELECT category_name, usage_count FROM category_usage WHERE guild_id = $1")
            .bind(gid)
            .fetch_all(pool)
            .await?;

    let mut category_usage: HashMap<String, u64> = HashMap::new();
    for row in usage_rows {
        let cat: String = row.get("category_name");
        let count: i64 = row.get("usage_count");
        category_usage.insert(cat, count.cast_unsigned());
    }

    Ok(GuildState {
        custom_categories,
        used_names,
        history,
        stats: GuildStats {
            total_changes,
            category_usage,
            bulk_randomize_count,
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
    let gid = guild_id.get().cast_signed();
    sqlx::query(
        r"
        INSERT INTO custom_categories (guild_id, name, items)
        VALUES ($1, $2, $3)
        ON CONFLICT (guild_id, name) DO UPDATE SET items = EXCLUDED.items
        ",
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
    let gid = guild_id.get().cast_signed();
    sqlx::query("DELETE FROM custom_categories WHERE guild_id = $1 AND name = $2")
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
    let gid = guild_id.get().cast_signed();
    sqlx::query(
        r"
        INSERT INTO used_names (guild_id, category_name, name)
        VALUES ($1, $2, $3)
        ON CONFLICT DO NOTHING
        ",
    )
    .bind(gid)
    .bind(category)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Bulk-mark many `(category, name)` pairs as used in a single round trip.
pub async fn add_used_names_bulk(
    pool: &PgPool,
    guild_id: GuildId,
    pairs: &[(String, String)],
) -> Result<(), sqlx::Error> {
    if pairs.is_empty() {
        return Ok(());
    }
    let gid = guild_id.get().cast_signed();
    let cats: Vec<String> = pairs.iter().map(|(c, _)| c.clone()).collect();
    let names: Vec<String> = pairs.iter().map(|(_, n)| n.clone()).collect();
    sqlx::query(
        r"
        INSERT INTO used_names (guild_id, category_name, name)
        SELECT $1, c, n FROM UNNEST($2::text[], $3::text[]) AS t(c, n)
        ON CONFLICT DO NOTHING
        ",
    )
    .bind(gid)
    .bind(&cats)
    .bind(&names)
    .execute(pool)
    .await?;
    Ok(())
}

/// One recorded nickname change, used for bulk persistence.
pub struct NickChangeRecord {
    pub user_id: u64,
    pub user_name: String,
    pub old_nick: Option<String>,
    pub new_nick: String,
    pub category: String,
}

/// Bulk-insert many nick-change rows in a single round trip.
pub async fn insert_nick_changes_bulk(
    pool: &PgPool,
    guild_id: GuildId,
    rows: &[NickChangeRecord],
) -> Result<(), sqlx::Error> {
    if rows.is_empty() {
        return Ok(());
    }
    let gid = guild_id.get().cast_signed();
    let uids: Vec<i64> = rows.iter().map(|r| r.user_id.cast_signed()).collect();
    let unames: Vec<String> = rows.iter().map(|r| r.user_name.clone()).collect();
    let olds: Vec<Option<String>> = rows.iter().map(|r| r.old_nick.clone()).collect();
    let news: Vec<String> = rows.iter().map(|r| r.new_nick.clone()).collect();
    let cats: Vec<String> = rows.iter().map(|r| r.category.clone()).collect();
    sqlx::query(
        r"
        INSERT INTO nick_changes (guild_id, user_id, user_name, old_nick, new_nick, category)
        SELECT $1, uid, un, old, nn, cat
        FROM UNNEST($2::bigint[], $3::text[], $4::text[], $5::text[], $6::text[])
            AS t(uid, un, old, nn, cat)
        ",
    )
    .bind(gid)
    .bind(&uids)
    .bind(&unames)
    .bind(&olds)
    .bind(&news)
    .bind(&cats)
    .execute(pool)
    .await?;
    Ok(())
}

/// Bulk-increment category usage counters from a `(category, delta)` list.
pub async fn increment_category_usage_bulk(
    pool: &PgPool,
    guild_id: GuildId,
    counts: &[(String, i64)],
) -> Result<(), sqlx::Error> {
    if counts.is_empty() {
        return Ok(());
    }
    let gid = guild_id.get().cast_signed();
    let cats: Vec<String> = counts.iter().map(|(c, _)| c.clone()).collect();
    let deltas: Vec<i64> = counts.iter().map(|(_, n)| *n).collect();
    sqlx::query(
        r"
        INSERT INTO category_usage (guild_id, category_name, usage_count)
        SELECT $1, c, n FROM UNNEST($2::text[], $3::bigint[]) AS t(c, n)
        ON CONFLICT (guild_id, category_name) DO UPDATE
            SET usage_count = category_usage.usage_count + EXCLUDED.usage_count
        ",
    )
    .bind(gid)
    .bind(&cats)
    .bind(&deltas)
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
    let gid = guild_id.get().cast_signed();
    match category {
        Some(cat) => {
            sqlx::query("DELETE FROM used_names WHERE guild_id = $1 AND category_name = $2")
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

/// Return each user's *original* nickname for this guild — the `old_nick`
/// recorded on the earliest nick-change row for that user. `None` means the
/// user had no nickname before the bot first touched them (so restoring
/// should clear their nickname). Used by `/restore`.
pub async fn original_nicks(
    pool: &PgPool,
    guild_id: GuildId,
    user_id: Option<u64>,
) -> Result<Vec<(u64, Option<String>)>, sqlx::Error> {
    let gid = guild_id.get().cast_signed();
    let rows = match user_id {
        Some(uid) => {
            sqlx::query(
                r"
                SELECT DISTINCT ON (user_id) user_id, old_nick
                FROM nick_changes
                WHERE guild_id = $1 AND user_id = $2
                ORDER BY user_id, changed_at ASC
                ",
            )
            .bind(gid)
            .bind(uid.cast_signed())
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                r"
                SELECT DISTINCT ON (user_id) user_id, old_nick
                FROM nick_changes
                WHERE guild_id = $1
                ORDER BY user_id, changed_at ASC
                ",
            )
            .bind(gid)
            .fetch_all(pool)
            .await?
        }
    };

    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.get::<i64, _>("user_id").cast_unsigned(),
                r.get::<Option<String>, _>("old_nick"),
            )
        })
        .collect())
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
    let gid = guild_id.get().cast_signed();
    let uid = user_id.cast_signed();
    sqlx::query(
        r"
        INSERT INTO nick_changes (guild_id, user_id, user_name, old_nick, new_nick, category)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
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
    let gid = guild_id.get().cast_signed();
    sqlx::query(
        r"
        INSERT INTO guild_stats (guild_id, total_changes, bulk_randomize_count)
        VALUES ($1, $2, $3)
        ON CONFLICT (guild_id) DO UPDATE
            SET total_changes        = EXCLUDED.total_changes,
                bulk_randomize_count = EXCLUDED.bulk_randomize_count
        ",
    )
    .bind(gid)
    .bind(total_changes.cast_signed())
    .bind(bulk_randomize_count.cast_signed())
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
    let gid = guild_id.get().cast_signed();
    sqlx::query(
        r"
        INSERT INTO category_usage (guild_id, category_name, usage_count)
        VALUES ($1, $2, 1)
        ON CONFLICT (guild_id, category_name) DO UPDATE
            SET usage_count = category_usage.usage_count + 1
        ",
    )
    .bind(gid)
    .bind(category)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Feedback lookups ─────────────────────────────────────────────────────────

/// A minimal projection of a `nick_changes` row used by the feedback flow:
/// just the fields needed to render the prompt and to attach feedback by FK.
#[derive(Debug, Clone)]
pub struct RecentNickChange {
    pub id: i64,
    pub category: String,
    pub new_nick: String,
    pub changed_at: DateTime<Utc>,
}

/// Most recent `nick_changes` row for `(guild_id, user_id)` within the last
/// `days` days, or `None` if no qualifying row exists.
pub async fn find_recent_nick_change(
    pool: &PgPool,
    guild_id: GuildId,
    user_id: u64,
    days: i32,
) -> Result<Option<RecentNickChange>, sqlx::Error> {
    let row = sqlx::query(
        r"
        SELECT id, category, new_nick, changed_at
        FROM nick_changes
        WHERE guild_id = $1
          AND user_id  = $2
          AND changed_at > NOW() - ($3::int * INTERVAL '1 day')
        ORDER BY changed_at DESC
        LIMIT 1
        ",
    )
    .bind(guild_id.get().cast_signed())
    .bind(user_id.cast_signed())
    .bind(days)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| RecentNickChange {
        id: r.get("id"),
        category: r.get("category"),
        new_nick: r.get("new_nick"),
        changed_at: r.get("changed_at"),
    }))
}

/// Insert or update one feedback row for `(nick_change_id, submitted_by)`.
/// Resubmissions overwrite prior values and refresh `created_at` to act
/// like a "last edited at" — this keeps the table small and prevents
/// duplicate feedback per assignment from one user.
pub async fn upsert_feedback(
    pool: &PgPool,
    nick_change_id: i64,
    submitted_by: u64,
    is_relevant: Option<bool>,
    nsfw_miscategorized: bool,
    note: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO feedback
            (nick_change_id, submitted_by, is_relevant, nsfw_miscategorized, note)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (nick_change_id, submitted_by) DO UPDATE
            SET is_relevant         = EXCLUDED.is_relevant,
                nsfw_miscategorized = EXCLUDED.nsfw_miscategorized,
                note                = EXCLUDED.note,
                created_at          = NOW()
        ",
    )
    .bind(nick_change_id)
    .bind(submitted_by.cast_signed())
    .bind(is_relevant)
    .bind(nsfw_miscategorized)
    .bind(note)
    .execute(pool)
    .await?;
    Ok(())
}
