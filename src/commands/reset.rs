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
    #[description = "Category to reset (omit to reset all categories)"] category: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    {
        let mut data = ctx.data().write().await;
        let gs = data.guild_mut(guild_id);
        gs.reset_pool(category.as_deref());
    }

    match &category {
        Some(cat) => {
            ctx.say(format!("🔄 Reset name pool for category **{}**.", cat))
                .await?
        }
        None => ctx.say("🔄 Reset name pools for **all** categories.").await?,
    };

    Ok(())
}
