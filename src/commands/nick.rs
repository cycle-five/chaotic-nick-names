use poise::serenity_prelude as serenity;

use crate::commands::randomize::{resolve_category, truncate_nick};
use crate::{Context, Error};

/// Assign a random nickname to a specific user.
///
/// Requires the **Manage Nicknames** permission.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_NICKNAMES",
    description_localized("en-US", "Assign a random nickname to a specific user")
)]
pub async fn nick(
    ctx: Context<'_>,
    #[description = "The user to rename"] user: serenity::User,
    #[description = "Category to pick a name from (omit for a random category)"]
    category: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();

    // Look up the guild member so we can read their current nickname
    let member = guild_id.member(&http, user.id).await?;

    // Determine category + name list
    let (cat_name, names) = {
        let data = ctx.data().read().await;
        let categories = match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => crate::data::builtin_categories(),
        };
        match resolve_category(&categories, category.as_deref()) {
            Ok(pair) => pair,
            Err(e) => {
                ctx.say(e.to_string()).await?;
                return Ok(());
            }
        }
    };

    // Draw a name (without-replacement)
    let new_nick = {
        let mut data = ctx.data().write().await;
        data.guild_mut(guild_id)
            .pick_name(&cat_name, &names)
            .ok_or("Category has no names")?
    };

    let nick = truncate_nick(&new_nick).to_string();
    let old_nick = member.nick.clone();

    guild_id
        .edit_member(&http, user.id, serenity::EditMember::new().nickname(&nick))
        .await?;

    {
        let mut data = ctx.data().write().await;
        data.guild_mut(guild_id).record_change(
            user.id.get(),
            user.name.clone(),
            old_nick,
            new_nick.clone(),
            cat_name.clone(),
        );
    }

    ctx.say(format!(
        "✅ Renamed **{}** to **{}** (from the **{}** category).",
        user.name, new_nick, cat_name
    ))
    .await?;

    Ok(())
}
