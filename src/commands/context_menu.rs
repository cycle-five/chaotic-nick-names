use poise::serenity_prelude as serenity;

use crate::commands::randomize::{escape_mentions, resolve_category, truncate_nick};
use crate::{Data, Error};

/// Modal shown when a user right-clicks → **Assign Random Nick**.
#[derive(Debug, poise::Modal)]
#[name = "Assign Nickname Options"]
struct NickModal {
    #[name = "Category"]
    #[placeholder = "Leave blank for a random category (e.g. scientists)"]
    #[min_length = 0]
    #[max_length = 64]
    category: Option<String>,

    #[name = "Specific name"]
    #[placeholder = "Leave blank to pick randomly from the category"]
    #[min_length = 0]
    #[max_length = 32]
    specific_name: Option<String>,
}

/// Right-click a user → **Assign Random Nick** to give them a nickname.
///
/// A modal lets you optionally choose a category and/or a specific name.
/// Requires the **Manage Nicknames** permission.
#[poise::command(
    context_menu_command = "Assign Random Nick",
    guild_only,
    required_permissions = "MANAGE_NICKNAMES"
)]
pub async fn assign_random_nick(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    user: serenity::User,
) -> Result<(), Error> {
    // Show the modal to optionally pick a category / specific name
    let modal = poise::execute_modal(ctx, None::<NickModal>, None).await?;
    let NickModal { category, specific_name } = match modal {
        Some(m) => m,
        None => return Ok(()), // user dismissed
    };

    // Normalise inputs
    let cat_input = category.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let name_input = specific_name.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();

    // Resolve category + name list
    let (cat_name, names) = {
        let data = ctx.data.read_state().await;
        let categories = match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => crate::data::builtin_categories(),
        };
        match resolve_category(&categories, cat_input) {
            Ok(pair) => pair,
            Err(e) => {
                ctx.say(e.to_string()).await?;
                return Ok(());
            }
        }
    };

    // Pick the name (specific or random without-replacement)
    let new_nick = if let Some(req) = name_input {
        let result = {
            let mut data = ctx.data.write_state().await;
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
        let mut data = ctx.data.write_state().await;
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

    let (total_ch, bulk_ct) = {
        let mut data = ctx.data.write_state().await;
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
        let db = ctx.data.db.clone();
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
    ctx
        .send(
            poise::CreateReply::default()
                .content(format!(
                    "✅ Renamed **{}** to **{}** (from the **{}** category).",
                    user.name, safe_nick, cat_name
                ))
                .allowed_mentions(serenity::CreateAllowedMentions::new().empty_parse()),
        )
        .await?;

    Ok(())
}
