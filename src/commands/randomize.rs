use poise::serenity_prelude as serenity;

use crate::{Context, Error};

/// Assign a random nickname from a category to every member of the server.
///
/// Names are chosen without replacement — the full pool must be exhausted before
/// any name is repeated.  Requires the **Manage Nicknames** permission.
#[poise::command(
    slash_command,
    guild_only,
    required_permissions = "MANAGE_NICKNAMES",
    description_localized("en-US", "Assign random nicknames to every server member")
)]
pub async fn randomize(
    ctx: Context<'_>,
    #[description = "Category to pick names from (omit for a random category)"]
    category: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();

    // Collect members (up to 1,000 per page; paginated for larger servers)
    let members = fetch_all_members(guild_id, &http).await?;

    // Determine the category and its name list
    let (cat_name, names) = {
        let data = ctx.data().read().await;
        let categories = match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => crate::data::builtin_categories(),
        };
        resolve_category(&categories, category.as_deref())?
    };

    // Assign names (without-replacement draw, holding write lock)
    let assignments: Vec<(serenity::Member, String)> = {
        let mut data = ctx.data().write().await;
        let gs = data.guild_mut(guild_id);
        members
            .iter()
            .filter(|m| !m.user.bot)
            .filter_map(|m| gs.pick_name(&cat_name, &names).map(|n| (m.clone(), n)))
            .collect()
    };

    let total = assignments.len();
    let mut changed = 0u32;
    let mut errors = 0u32;

    for (member, new_nick) in &assignments {
        let nick = truncate_nick(new_nick);
        match guild_id
            .edit_member(&http, member.user.id, serenity::EditMember::new().nickname(nick))
            .await
        {
            Ok(_) => {
                changed += 1;
                let mut data = ctx.data().write().await;
                data.guild_mut(guild_id).record_change(
                    member.user.id.get(),
                    member.user.name.clone(),
                    member.nick.clone(),
                    new_nick.clone(),
                    cat_name.clone(),
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Could not change nick for {} in {}: {:?}",
                    member.user.name,
                    guild_id,
                    e
                );
                errors += 1;
            }
        }
        // Respect Discord rate-limits (~5 req/s per endpoint)
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
    }

    {
        let mut data = ctx.data().write().await;
        data.guild_mut(guild_id).stats.bulk_randomize_count += 1;
    }

    ctx.say(format!(
        "🎲 Randomized **{}** member(s) from the **{}** category.\n\
         ✅ Changed: **{}** | ❌ Skipped/errors: **{}**",
        total, cat_name, changed, errors
    ))
    .await?;

    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Fetch up to `limit` members per page, paginating until exhausted.
pub async fn fetch_all_members(
    guild_id: serenity::GuildId,
    http: &serenity::Http,
) -> Result<Vec<serenity::Member>, Error> {
    let mut all: Vec<serenity::Member> = Vec::new();
    let mut after: Option<serenity::UserId> = None;
    loop {
        let page = guild_id.members(http, Some(1000), after).await?;
        let done = page.len() < 1000;
        let last = page.last().map(|m| m.user.id);
        all.extend(page);
        if done {
            break;
        }
        after = last;
    }
    Ok(all)
}

/// Choose a category, returning `(name, names_vec)`.
/// If `requested` is `None` a random category is chosen.
pub fn resolve_category(
    categories: &std::collections::HashMap<String, Vec<String>>,
    requested: Option<&str>,
) -> Result<(String, Vec<String>), Error> {
    if let Some(req) = requested {
        let key = req.to_lowercase();
        if let Some(names) = categories.get(&key) {
            return Ok((key, names.clone()));
        }
        let available = {
            let mut keys: Vec<&String> = categories.keys().collect();
            keys.sort();
            keys.iter().map(|k| format!("`{}`", k)).collect::<Vec<_>>().join(", ")
        };
        Err(format!("Unknown category `{}`. Available: {}", req, available).into())
    } else {
        use rand::seq::IteratorRandom;
        let mut rng = rand::thread_rng();
        let (k, v) = categories
            .iter()
            .choose(&mut rng)
            .ok_or("No categories available")?;
        Ok((k.clone(), v.clone()))
    }
}

/// Truncate a potential nickname to Discord's 32-character limit,
/// respecting UTF-8 character boundaries.
pub fn truncate_nick(s: &str) -> &str {
    if s.len() <= 32 {
        return s;
    }
    let mut idx = 32;
    while !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}
