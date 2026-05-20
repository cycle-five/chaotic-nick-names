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
///
/// Permissions come from `Member.permissions` populated by Discord on the
/// interaction payload. Discord computes this server-side with guild owner
/// all-perms and channel-level overrides applied — no cache lookup, no
/// cold-start failure mode. Channel overrides for the perms we gate on
/// (Manage Nicknames, Manage Server) are pathological config and we accept
/// whatever Discord reports.
pub async fn require_permission(
    ctx: Context<'_>,
    needed: serenity::Permissions,
    label: &str,
) -> Result<bool, Error> {
    let perms = ctx.author_member().await.and_then(|m| m.permissions);
    let Some(perms) = perms else {
        // guild_only application commands always have a populated member
        // with permissions; if we ever lack one, something is wrong with
        // the interaction shape — fail closed.
        tracing::warn!(
            "no permissions on interaction for user {} in guild {:?}",
            ctx.author().id.get(),
            ctx.guild_id(),
        );
        ctx.send(
            poise::CreateReply::default()
                .content("Couldn't verify your permissions — try again in a moment.")
                .ephemeral(true),
        )
        .await?;
        return Ok(false);
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
    require_permission(
        ctx,
        serenity::Permissions::MANAGE_NICKNAMES,
        "Manage Nicknames",
    )
    .await
}

/// Shorthand for [`require_permission`] checking `Manage Server` (the
/// Discord permission flag is named `MANAGE_GUILD` in the API).
pub async fn require_manage_guild(ctx: Context<'_>) -> Result<bool, Error> {
    require_permission(ctx, serenity::Permissions::MANAGE_GUILD, "Manage Server").await
}
