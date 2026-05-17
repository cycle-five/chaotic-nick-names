use std::time::Duration;

use poise::serenity_prelude as serenity;

use crate::{Context, Error};

/// Assign a random nickname to every member of the server.
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
    #[autocomplete = "autocomplete_category"]
    category: Option<String>,
    #[description = "Full chaos: give every member a name from a DIFFERENT random category"]
    chaos: Option<bool>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();
    let chaos = chaos.unwrap_or(false);

    // Collect members (up to 1,000 per page; paginated for larger servers)
    let members = fetch_all_members(guild_id, &http).await?;
    let member_count = members.iter().filter(|m| !m.user.bot).count();

    if member_count == 0 {
        ctx.say("There are no non-bot members to randomize.").await?;
        return Ok(());
    }

    // Snapshot the category map once.
    let categories = {
        let data = ctx.data().read_state().await;
        match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => crate::data::builtin_categories(),
        }
    };

    // In chaos mode we draw a fresh category per member; otherwise resolve one.
    let resolved = if chaos {
        None
    } else {
        match resolve_category(&categories, category.as_deref()) {
            Ok(pair) => Some(pair),
            Err(e) => {
                ctx.say(e.to_string()).await?;
                return Ok(());
            }
        }
    };

    let scope_label = match &resolved {
        Some((name, _)) => format!("the **{}** category", name),
        None => "**random categories** (full chaos)".to_string(),
    };

    // ── Confirmation step ─────────────────────────────────────────────────────
    let confirm_id = format!("cnn-confirm-{}", ctx.id());
    let cancel_id = format!("cnn-cancel-{}", ctx.id());
    let prompt = ctx
        .send(
            poise::CreateReply::default()
                .content(format!(
                    "⚠️ This will rename **{}** member(s) using {}.\nThis cannot be \
                     automatically undone except via `/restore`. Proceed?",
                    member_count, scope_label
                ))
                .components(vec![serenity::CreateActionRow::Buttons(vec![
                    serenity::CreateButton::new(&confirm_id)
                        .label("Randomize everyone")
                        .style(serenity::ButtonStyle::Danger),
                    serenity::CreateButton::new(&cancel_id)
                        .label("Cancel")
                        .style(serenity::ButtonStyle::Secondary),
                ])]),
        )
        .await?;

    let interaction = serenity::ComponentInteractionCollector::new(ctx)
        .author_id(ctx.author().id)
        .message_id(prompt.message().await?.id)
        .timeout(Duration::from_secs(60))
        .await;

    let confirmed = match interaction {
        Some(mci) if mci.data.custom_id == confirm_id => {
            mci.create_response(ctx.serenity_context(), serenity::CreateInteractionResponse::Acknowledge)
                .await?;
            true
        }
        Some(mci) => {
            mci.create_response(ctx.serenity_context(), serenity::CreateInteractionResponse::Acknowledge)
                .await?;
            false
        }
        None => false,
    };

    if !confirmed {
        prompt
            .edit(
                ctx,
                poise::CreateReply::default()
                    .content("❌ Randomize cancelled.")
                    .components(vec![]),
            )
            .await?;
        return Ok(());
    }

    // ── Assign names (without-replacement draw, holding write lock briefly) ────
    // Each tuple is (member, new_nick, category_used).
    // Only the fields we actually need downstream — avoids cloning the whole
    // (large) serenity::Member per member.
    struct Assignment {
        user_id: serenity::UserId,
        user_name: String,
        old_nick: Option<String>,
        new_nick: String,
        category: String,
    }

    let assignments: Vec<Assignment> = {
        // In chaos mode collect the category keys once (O(C)) so per-member
        // selection is O(1) instead of O(C) per member.
        let chaos_keys: Vec<&String> = if resolved.is_none() {
            categories.keys().collect()
        } else {
            Vec::new()
        };

        let mut data = ctx.data().write_state().await;
        let gs = data.guild_mut(guild_id);
        // Created after the lock is held and never kept across an `.await`
        // (ThreadRng is !Send); the closure below contains no await points.
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        members
            .iter()
            .filter(|m| !m.user.bot)
            .filter_map(|m| {
                let (cat, names) = match &resolved {
                    Some((c, n)) => (c.clone(), n.clone()),
                    None => {
                        let k = chaos_keys.choose(&mut rng)?;
                        (k.to_string(), categories[*k].clone())
                    }
                };
                gs.pick_name(&cat, &names).map(|new_nick| Assignment {
                    user_id: m.user.id,
                    user_name: m.user.name.clone(),
                    old_nick: m.nick.clone(),
                    new_nick,
                    category: cat,
                })
            })
            .collect()
    };

    // Persist the pool updates to DB in a single bulk insert (best-effort).
    {
        let pairs: Vec<(String, String)> = assignments
            .iter()
            .map(|a| (a.category.clone(), a.new_nick.clone()))
            .collect();
        if let Err(e) = crate::db::add_used_names_bulk(&ctx.data().db, guild_id, &pairs).await {
            tracing::warn!("DB error bulk-persisting used_names: {:?}", e);
        }
    }

    let total = assignments.len();
    let channel_id = ctx.channel_id();
    let bot_data = ctx.data().clone();

    prompt
        .edit(
            ctx,
            poise::CreateReply::default()
                .content(format!(
                    "🎲 Randomizing **{}** member(s) using {} — this may take a moment…",
                    total, scope_label
                ))
                .components(vec![]),
        )
        .await?;

    // Apply edits in the background; Serenity's built-in rate-limit handling
    // manages pacing automatically, removing the need for a fixed sleep.
    tokio::spawn(async move {
        let mut changed = 0u32;
        let mut errors = 0u32;
        let mut change_rows: Vec<crate::db::NickChangeRecord> = Vec::new();
        let mut usage: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

        for a in &assignments {
            let nick = truncate_nick(&a.new_nick);
            match guild_id
                .edit_member(&http, a.user_id, serenity::EditMember::new().nickname(nick))
                .await
            {
                Ok(_) => {
                    changed += 1;
                    {
                        let mut data = bot_data.write_state().await;
                        data.guild_mut(guild_id).record_change(
                            a.user_id.get(),
                            a.user_name.clone(),
                            a.old_nick.clone(),
                            a.new_nick.clone(),
                            a.category.clone(),
                        );
                    }
                    *usage.entry(a.category.clone()).or_insert(0) += 1;
                    change_rows.push(crate::db::NickChangeRecord {
                        user_id: a.user_id.get(),
                        user_name: a.user_name.clone(),
                        old_nick: a.old_nick.clone(),
                        new_nick: a.new_nick.clone(),
                        category: a.category.clone(),
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        "Could not change nick for {} in {}: {:?}",
                        a.user_name,
                        guild_id,
                        e
                    );
                    errors += 1;
                }
            }
        }

        // Persist all changes for this run in a few bulk round trips instead of
        // a query (and a spawned task) per member.
        let _ = crate::db::insert_nick_changes_bulk(&bot_data.db, guild_id, &change_rows).await;
        let usage_list: Vec<(String, i64)> = usage.into_iter().collect();
        let _ = crate::db::increment_category_usage_bulk(&bot_data.db, guild_id, &usage_list).await;

        // Increment the bulk-run counter, then persist the final aggregate
        // stats exactly once (this also fixes bulk_randomize_count never
        // reaching the database).
        let (total_ch, bulk_ct) = {
            let mut data = bot_data.write_state().await;
            let gs = data.guild_mut(guild_id);
            gs.stats.bulk_randomize_count += 1;
            (gs.stats.total_changes, gs.stats.bulk_randomize_count)
        };
        let _ = crate::db::upsert_guild_stats(&bot_data.db, guild_id, total_ch, bulk_ct).await;

        tracing::info!(
            guild = %guild_id,
            changed,
            errors,
            "Background randomize task completed"
        );
        let summary = format!(
            "✅ Randomization complete! Changed: **{}** | ❌ Skipped/errors: **{}**",
            changed, errors
        );
        if let Err(e) = channel_id
            .send_message(
                &http,
                serenity::CreateMessage::new()
                    .content(summary)
                    .allowed_mentions(serenity::CreateAllowedMentions::new()),
            )
            .await
        {
            tracing::warn!("Failed to send randomize summary to channel {}: {:?}", channel_id, e);
        }
    });

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
    // Fast-path: most built-in names are well under 32 chars
    if s.chars().count() <= 32 {
        return s;
    }
    if let Some((idx, _)) = s.char_indices().nth(32) {
        &s[..idx]
    } else {
        s
    }
}

/// Escape `@` to prevent unintended mention pings when echoing user-controlled text.
pub fn escape_mentions(s: &str) -> String {
    s.replace('@', "@\u{200b}")
}

/// Friendly, actionable message shown when the bot cannot edit a member's
/// nickname (almost always role hierarchy / ownership / missing permission).
pub fn nick_edit_failure_message(user_name: &str) -> String {
    format!(
        "❌ Couldn't change **{}**'s nickname. This usually means they have a \
         role higher than mine, they're the server owner, or I'm missing the \
         **Manage Nicknames** permission.",
        escape_mentions(user_name)
    )
}

/// Autocomplete handler for the `category` option of several commands.
/// Includes this guild's custom categories alongside the built-ins.
pub async fn autocomplete_category(
    ctx: Context<'_>,
    partial: &str,
) -> impl Iterator<Item = String> {
    let partial = partial.to_lowercase();
    let mut names: Vec<String> = if let Some(gid) = ctx.guild_id() {
        let data = ctx.data().read_state().await;
        match data.guild(gid) {
            Some(gs) => gs.all_categories().into_keys().collect(),
            None => crate::data::builtin_category_names(),
        }
    } else {
        crate::data::builtin_category_names()
    };
    names.sort();
    names
        .into_iter()
        .filter(move |c| c.starts_with(&partial))
        .take(25)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── truncate_nick ─────────────────────────────────────────────────────────

    #[test]
    fn truncate_nick_short_string_unchanged() {
        assert_eq!(truncate_nick("Einstein"), "Einstein");
    }

    #[test]
    fn truncate_nick_exactly_32_chars_unchanged() {
        let s = "a".repeat(32);
        assert_eq!(truncate_nick(&s), s.as_str());
    }

    #[test]
    fn truncate_nick_long_string_capped_at_32() {
        let s = "a".repeat(50);
        let result = truncate_nick(&s);
        assert_eq!(result.chars().count(), 32);
    }

    #[test]
    fn truncate_nick_respects_utf8_boundaries() {
        // Each 'é' is 2 bytes; the function counts codepoints, not bytes.
        let s = "é".repeat(40);
        let result = truncate_nick(&s);
        assert_eq!(result.chars().count(), 32);
    }

    #[test]
    fn truncate_nick_one_over_limit() {
        let s = "x".repeat(33);
        assert_eq!(truncate_nick(&s).chars().count(), 32);
    }

    // ── escape_mentions ───────────────────────────────────────────────────────

    #[test]
    fn escape_mentions_inserts_zwsp_after_at() {
        let result = escape_mentions("@everyone");
        assert!(result.contains('\u{200b}'));
        assert!(result.starts_with('@'));
    }

    #[test]
    fn escape_mentions_no_at_unchanged() {
        let input = "hello world";
        assert_eq!(escape_mentions(input), input);
    }

    #[test]
    fn escape_mentions_multiple_at_signs() {
        let result = escape_mentions("@a @b");
        assert_eq!(result.matches('\u{200b}').count(), 2);
    }

    #[test]
    fn escape_mentions_neutralizes_here() {
        // @here must not survive intact as a working ping.
        assert!(!escape_mentions("@here").contains("@h"));
    }

    // ── nick_edit_failure_message ─────────────────────────────────────────────

    #[test]
    fn failure_message_mentions_user_and_guidance() {
        let m = nick_edit_failure_message("Alice");
        assert!(m.contains("Alice"));
        assert!(m.contains("Manage Nicknames"));
    }

    #[test]
    fn failure_message_escapes_at_in_username() {
        let m = nick_edit_failure_message("@everyone");
        assert!(m.contains('\u{200b}'));
    }

    // ── resolve_category ──────────────────────────────────────────────────────

    fn sample_categories() -> HashMap<String, Vec<String>> {
        let mut m = HashMap::new();
        m.insert(
            "scientists".to_string(),
            vec!["Einstein".to_string(), "Newton".to_string()],
        );
        m.insert(
            "planets".to_string(),
            vec!["Mars".to_string(), "Venus".to_string()],
        );
        m
    }

    #[test]
    fn resolve_category_known_name_returns_it() {
        let cats = sample_categories();
        let (name, names) = resolve_category(&cats, Some("scientists")).unwrap();
        assert_eq!(name, "scientists");
        assert!(names.contains(&"Einstein".to_string()));
    }

    #[test]
    fn resolve_category_is_case_insensitive() {
        let cats = sample_categories();
        let (name, _) = resolve_category(&cats, Some("SCIENTISTS")).unwrap();
        assert_eq!(name, "scientists");
    }

    #[test]
    fn resolve_category_unknown_name_returns_error() {
        let cats = sample_categories();
        let result = resolve_category(&cats, Some("dinosaurs"));
        assert!(result.is_err());
    }

    #[test]
    fn resolve_category_unknown_name_lists_available() {
        let cats = sample_categories();
        let err = resolve_category(&cats, Some("nope")).unwrap_err().to_string();
        assert!(err.contains("scientists"));
        assert!(err.contains("planets"));
    }

    #[test]
    fn resolve_category_none_picks_a_random_category() {
        let cats = sample_categories();
        let (name, names) = resolve_category(&cats, None).unwrap();
        assert!(cats.contains_key(&name));
        assert!(!names.is_empty());
    }

    #[test]
    fn resolve_category_empty_map_returns_error() {
        let cats: HashMap<String, Vec<String>> = HashMap::new();
        assert!(resolve_category(&cats, None).is_err());
    }
}
