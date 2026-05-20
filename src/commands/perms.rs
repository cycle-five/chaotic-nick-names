//! Runtime permission checks.
//!
//! These replace poise's `#[poise::command(required_permissions = "...")]`
//! attribute for the gated commands. The attribute-based check was producing
//! confusing UX because poise runs it *before* autocomplete handlers and
//! before the command body — so users without the perm saw an empty
//! autocomplete list (which looks like a bot bug) and got a vague
//! "you may be lacking permissions" message on execute. Some legitimate
//! permission holders (including the server owner in at least one observed
//! case) were also rejected.
//!
//! The manual checks below run *inside* the command body, so:
//!
//! - autocomplete works regardless of permission state,
//! - the rejection message names the specific permission required, and
//! - we compute permissions via serenity, which correctly grants the guild
//!   owner all permissions.

use poise::serenity_prelude as serenity;

use crate::{Context, Error};

/// Generic engine: check the invoker's permissions in the current guild
/// against `needed`. On success returns `Ok(true)`. On failure sends a
/// clear ephemeral message and returns `Ok(false)`; callers should
/// `return Ok(())` from the command in that case.
///
/// `label` is the human-readable name shown in the rejection message
/// (e.g. "Manage Nicknames").
pub async fn require_permission(
    ctx: Context<'_>,
    needed: serenity::Permissions,
    label: &str,
) -> Result<bool, Error> {
    let Some(member) = ctx.author_member().await else {
        // Should be impossible on `guild_only` commands, but bail safely.
        ctx.send(
            poise::CreateReply::default()
                .content("This command must be used inside a server.")
                .ephemeral(true),
        )
        .await?;
        return Ok(false);
    };

    // `Member::permissions` is deprecated in favour of
    // `Guild::user_permissions_in`, which additionally applies channel-level
    // permission overrides. We intentionally don't want that: Manage
    // Nicknames and Manage Server are server-wide concerns (nicknames are
    // not channel-scoped, custom-category writes affect the whole guild), so
    // the guild-wide computation is exactly the semantics we want. Switching
    // would also require plumbing channel resolution with a thread fallback.
    #[allow(deprecated)]
    let perms = match member.permissions(ctx.cache()) {
        Ok(p) => p,
        Err(e) => {
            // Cache miss for the guild is the realistic failure mode. Fail
            // closed: tell the user to retry rather than risk silently
            // letting an unauthorised call through.
            tracing::warn!(
                "permission lookup failed for user {} in guild {:?}: {e}",
                ctx.author().id.get(),
                ctx.guild_id(),
            );
            ctx.send(
                poise::CreateReply::default()
                    .content(
                        "Couldn't verify your permissions for this command \
                         right now — try again in a moment.",
                    )
                    .ephemeral(true),
            )
            .await?;
            return Ok(false);
        }
    };

    if perms.contains(needed) {
        return Ok(true);
    }

    ctx.send(
        poise::CreateReply::default()
            .content(format!(
                "🔒 This command requires the **{label}** permission. \
                 Ask a server admin to grant it to one of your roles."
            ))
            .ephemeral(true),
    )
    .await?;
    Ok(false)
}

/// Shorthand for [`require_permission`] checking `Manage Nicknames`.
pub async fn require_manage_nicknames(ctx: Context<'_>) -> Result<bool, Error> {
    require_permission(ctx, serenity::Permissions::MANAGE_NICKNAMES, "Manage Nicknames").await
}

/// Shorthand for [`require_permission`] checking `Manage Server` (the
/// Discord permission flag is named `MANAGE_GUILD` in the API).
pub async fn require_manage_guild(ctx: Context<'_>) -> Result<bool, Error> {
    require_permission(ctx, serenity::Permissions::MANAGE_GUILD, "Manage Server").await
}
