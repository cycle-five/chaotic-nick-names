use poise::serenity_prelude as serenity;

use crate::commands::randomize::{resolve_category, truncate_nick};
use crate::{Context, Error};

/// Right-click a user → **Assign Random Nick** to give them a random nickname.
///
/// A random category is chosen automatically.
/// Requires the **Manage Nicknames** permission.
#[poise::command(
    context_menu_command = "Assign Random Nick",
    guild_only,
    required_permissions = "MANAGE_NICKNAMES"
)]
pub async fn assign_random_nick(
    ctx: Context<'_>,
    user: serenity::User,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();

    // Determine a random category
    let (cat_name, names) = {
        let data = ctx.data().read().await;
        let categories = match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => crate::data::builtin_categories(),
        };
        match resolve_category(&categories, None) {
            Ok(pair) => pair,
            Err(e) => {
                ctx.say(e.to_string()).await?;
                return Ok(());
            }
        }
    };

    let new_nick = {
        let mut data = ctx.data().write().await;
        data.guild_mut(guild_id)
            .pick_name(&cat_name, &names)
            .ok_or("Category has no names")?
    };

    let nick = truncate_nick(&new_nick).to_string();

    // Fetch the current nickname for history
    let member = guild_id.member(&http, user.id).await?;
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
