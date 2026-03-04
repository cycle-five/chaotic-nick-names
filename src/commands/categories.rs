use crate::{data, Context, Error};

// ── /categories (parent with three sub-commands) ─────────────────────────────

/// Manage and view the nickname categories available in this server.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("list", "add", "remove"),
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
        let data = ctx.data().read().await;
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
    if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
        ctx.say("❌ Category name must be non-empty and contain only letters, digits, or underscores.").await?;
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
        let mut data = ctx.data().write().await;
        data.guild_mut(guild_id)
            .custom_categories
            .insert(key.clone(), parsed.clone());
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
        let mut data = ctx.data().write().await;
        data.guild_mut(guild_id)
            .custom_categories
            .remove(&key)
            .is_some()
    };

    if removed {
        ctx.say(format!("✅ Removed custom category **{}**.", key))
            .await?;
    } else {
        ctx.say(format!("❌ No custom category named `{}` found.", key))
            .await?;
    }

    Ok(())
}
