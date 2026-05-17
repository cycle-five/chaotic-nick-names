use crate::{Context, Error};

/// Reset the without-replacement name pool for a category (or all categories).
///
/// After resetting, names that were already assigned can be picked again.
/// Requires the **Manage Nicknames** permission.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_NICKNAMES",
    description_localized(
        "en-US",
        "Reset the name pool so previously used names become available again"
    )
)]
pub async fn reset_pool(
    ctx: Context<'_>,
    #[description = "Category to reset (omit to reset all categories)"]
    #[autocomplete = "crate::commands::randomize::autocomplete_category"]
    category: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    // Normalise to lowercase so it matches the stored keys
    let cat = category.as_deref().map(|s| s.to_lowercase());

    // Reject typos instead of silently reporting a successful reset.
    if let Some(c) = &cat {
        let known = {
            let data = ctx.data().read_state().await;
            match data.guild(guild_id) {
                Some(gs) => gs.all_categories().contains_key(c),
                None => crate::data::builtin_category_names().contains(c),
            }
        };
        if !known {
            ctx.say(format!("❌ No category named `{}` exists.", c))
                .await?;
            return Ok(());
        }
    }

    {
        let mut data = ctx.data().write_state().await;
        let gs = data.guild_mut(guild_id);
        gs.reset_pool(cat.as_deref());
    }

    // Persist to DB (best-effort)
    {
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let c = cat.clone();
        tokio::spawn(async move {
            let _ = crate::db::clear_used_names(&db, gid, c.as_deref()).await;
        });
    }

    match &cat {
        Some(c) => {
            ctx.say(format!("🔄 Reset name pool for category **{}**.", c))
                .await?
        }
        None => {
            ctx.say("🔄 Reset name pools for **all** categories.")
                .await?
        }
    };

    Ok(())
}
