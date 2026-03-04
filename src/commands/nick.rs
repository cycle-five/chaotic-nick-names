use poise::serenity_prelude as serenity;

use crate::commands::randomize::{escape_mentions, resolve_category, truncate_nick};
use crate::{Context, Error};

/// Assign a random (or specific) nickname to a specific user.
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
    #[description = "A specific name to assign (omit to pick randomly from the category)"]
    specific_name: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();

    // Look up the guild member so we can read their current nickname
    let member = guild_id.member(&http, user.id).await?;

    // Determine category + name list
    let (cat_name, names) = {
        let data = ctx.data().read_state().await;
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

    // Draw a name (without-replacement, or use the requested specific name)
    let new_nick = if let Some(ref req) = specific_name {
        let result = {
            let mut data = ctx.data().write_state().await;
            data.guild_mut(guild_id)
                .use_specific_name(&cat_name, req, &names)
        };
        match result {
            Ok(n) => n,
            Err(e) => {
                ctx.say(format!("❌ {}", e)).await?;
                return Ok(());
            }
        }
    } else {
        let mut data = ctx.data().write_state().await;
        data.guild_mut(guild_id)
            .pick_name(&cat_name, &names)
            .ok_or("Category has no names")?
    };

    let nick = truncate_nick(&new_nick).to_string();
    let old_nick = member.nick.clone();

    guild_id
        .edit_member(&http, user.id, serenity::EditMember::new().nickname(&nick))
        .await?;

    let (total_ch, bulk_ct) = {
        let mut data = ctx.data().write_state().await;
        data.guild_mut(guild_id).record_change(
            user.id.get(),
            user.name.clone(),
            old_nick.clone(),
            new_nick.clone(),
            cat_name.clone(),
        );
        let gs = data.guild(guild_id).unwrap();
        (gs.stats.total_changes, gs.stats.bulk_randomize_count)
    };

    // Persist to DB (best-effort)
    {
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let uid = user.id.get();
        let un = user.name.clone();
        let old = old_nick;
        let nn = new_nick.clone();
        let cn = cat_name.clone();
        tokio::spawn(async move {
            let _ = crate::db::add_used_name(&db, gid, &cn, &nn).await;
            let _ = crate::db::insert_nick_change(&db, gid, uid, &un, old.as_deref(), &nn, &cn).await;
            let _ = crate::db::upsert_guild_stats(&db, gid, total_ch, bulk_ct).await;
            let _ = crate::db::increment_category_usage(&db, gid, &cn).await;
        });
    }

    let safe_nick = escape_mentions(&new_nick);
    ctx.say(format!(
        "✅ Renamed **{}** to **{}** (from the **{}** category).",
        user.name, safe_nick, cat_name
    ))
    .await?;

    Ok(())
}
