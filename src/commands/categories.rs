use poise::serenity_prelude as serenity;

use crate::{data, Context, Error};

/// Validate a category key: must be non-empty, start with a letter, and contain
/// only letters, digits, or underscores.
fn valid_category_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().next().map_or(false, |c| c.is_alphabetic())
        && key.chars().all(|c| c.is_alphanumeric() || c == '_')
}

// ── /categories (parent with four sub-commands) ──────────────────────────────

/// Manage and view the nickname categories available in this server.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("list", "add", "remove", "import"),
    description_localized("en-US", "Manage nickname categories")
)]
pub async fn categories(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// List all available nickname categories.
#[poise::command(
    slash_command,
    guild_only,
    rename = "list",
    description_localized("en-US", "List all available nickname categories")
)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let all_cats = {
        let data = ctx.data().read_state().await;
        match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => data::builtin_categories(),
        }
    };

    let builtin_names = data::builtin_category_names();

    let mut lines: Vec<String> = all_cats
        .iter()
        .map(|(name, items)| {
            let tag = if builtin_names.contains(name) {
                ""
            } else {
                " *(custom)*"
            };
            format!("• **{}**{} — {} name(s)", name, tag, items.len())
        })
        .collect();
    lines.sort();

    ctx.say(format!(
        "**Available categories** ({} total)\n{}",
        all_cats.len(),
        lines.join("\n")
    ))
    .await?;

    Ok(())
}

/// Add a new custom category with a comma-separated list of names.
///
/// Requires the **Manage Guild** permission.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    rename = "add",
    description_localized("en-US", "Add a custom nickname category")
)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Category name (alphanumeric and underscores)"] name: String,
    #[description = "Comma-separated list of nickname values"] items: String,
) -> Result<(), Error> {
    // Validate name
    let key = name.to_lowercase();
    if !valid_category_key(&key) {
        ctx.say("❌ Category name must start with a letter and contain only letters, digits, or underscores.").await?;
        return Ok(());
    }

    let parsed: Vec<String> = items
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if parsed.is_empty() {
        ctx.say("❌ You must provide at least one name.").await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();

    {
        let mut data = ctx.data().write_state().await;
        let gs = data.guild_mut(guild_id);
        gs.custom_categories.insert(key.clone(), parsed.clone());
        // Clear the used-names pool so the updated list starts fresh
        gs.reset_pool(Some(&key));
    }

    // Persist to DB (best-effort)
    {
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let k = key.clone();
        let p = parsed.clone();
        tokio::spawn(async move {
            let _ = crate::db::upsert_custom_category(&db, gid, &k, &p).await;
            let _ = crate::db::clear_used_names(&db, gid, Some(&k)).await;
        });
    }

    ctx.say(format!(
        "✅ Added custom category **{}** with **{}** name(s).",
        key,
        parsed.len()
    ))
    .await?;

    Ok(())
}

/// Remove a custom category. Built-in categories cannot be removed.
///
/// Requires the **Manage Guild** permission.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    rename = "remove",
    description_localized("en-US", "Remove a custom nickname category")
)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Name of the custom category to remove"] name: String,
) -> Result<(), Error> {
    let key = name.to_lowercase();
    let builtin = data::builtin_category_names();

    if builtin.contains(&key) {
        ctx.say("❌ Built-in categories cannot be removed.").await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();
    let removed = {
        let mut data = ctx.data().write_state().await;
        let gs = data.guild_mut(guild_id);
        gs.remove_custom_category(&key)
    };

    if removed {
        // Persist to DB (best-effort)
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let k = key.clone();
        tokio::spawn(async move {
            let _ = crate::db::delete_custom_category(&db, gid, &k).await;
            let _ = crate::db::clear_used_names(&db, gid, Some(&k)).await;
        });

        ctx.say(format!("✅ Removed custom category **{}**.", key))
            .await?;
    } else {
        ctx.say(format!("❌ No custom category named `{}` found.", key))
            .await?;
    }

    Ok(())
}

/// Import categories from an attached CSV file.
///
/// Each row in the CSV must start with the **category name**, followed by the
/// names in that category:
/// ```
/// scientists,Einstein,Newton,Darwin,Curie
/// amusement_parks,Cedar Point,Dollywood,Alton Towers
/// ```
/// Multiple rows create or replace multiple categories at once.
/// Requires the **Manage Guild** permission.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_GUILD",
    rename = "import",
    description_localized("en-US", "Import categories from a CSV file attachment")
)]
pub async fn import(
    ctx: Context<'_>,
    #[description = "CSV file (each row: category_name,name1,name2,…)"]
    file: serenity::Attachment,
) -> Result<(), Error> {
    ctx.defer().await?;

    // Guard against oversized uploads (1 MiB limit)
    if file.size > 1_048_576 {
        ctx.say("❌ File too large (max 1 MiB).").await?;
        return Ok(());
    }

    let bytes = file.download().await?;
    let text = String::from_utf8(bytes)
        .map_err(|_| "❌ File is not valid UTF-8.")?;

    let guild_id = ctx.guild_id().unwrap();
    let mut added: Vec<(String, usize)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut fields: Vec<&str> = line.split(',').collect();
        if fields.is_empty() {
            continue;
        }

        let raw_key = fields.remove(0).trim();
        let key = raw_key.to_lowercase();

        if !valid_category_key(&key) {
            errors.push(format!(
                "Line {}: invalid category name `{}` (must start with a letter, alphanumeric and underscores only)",
                line_no + 1,
                raw_key
            ));
            continue;
        }

        let names: Vec<String> = fields
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if names.is_empty() {
            errors.push(format!("Line {}: category `{}` has no names", line_no + 1, key));
            continue;
        }

        let count = names.len();

        // Update in-memory state
        {
            let mut data = ctx.data().write_state().await;
            let gs = data.guild_mut(guild_id);
            gs.custom_categories.insert(key.clone(), names.clone());
            gs.reset_pool(Some(&key));
        }

        // Persist to DB (best-effort, inside the loop)
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let k = key.clone();
        let n = names.clone();
        tokio::spawn(async move {
            let _ = crate::db::upsert_custom_category(&db, gid, &k, &n).await;
            let _ = crate::db::clear_used_names(&db, gid, Some(&k)).await;
        });

        added.push((key, count));
    }

    let mut reply = String::new();
    if !added.is_empty() {
        let summary: Vec<String> = added
            .iter()
            .map(|(cat, n)| format!("• **{}** ({} name(s))", cat, n))
            .collect();
        reply.push_str(&format!(
            "✅ Imported **{}** categor{}:\n{}",
            added.len(),
            if added.len() == 1 { "y" } else { "ies" },
            summary.join("\n")
        ));
    }
    if !errors.is_empty() {
        if !reply.is_empty() {
            reply.push('\n');
        }
        reply.push_str(&format!(
            "⚠️ **{}** line(s) skipped:\n{}",
            errors.len(),
            errors.join("\n")
        ));
    }
    if reply.is_empty() {
        reply = "❌ No valid categories found in the file.".to_string();
    }

    ctx.say(reply).await?;
    Ok(())
}
