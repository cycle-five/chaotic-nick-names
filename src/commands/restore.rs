use poise::serenity_prelude as serenity;

use crate::commands::perms::require_manage_nicknames;
use crate::commands::randomize::truncate_nick;
use crate::{Context, Error};

/// Restore members' pre-bot nicknames (the undo for `/randomize`).
///
/// Pass a `user` to restore just that person; omit it to restore everyone the
/// bot has ever renamed in this server, using recorded history.
/// Requires the **Manage Nicknames** permission.
#[poise::command(
    slash_command,
    guild_only,
    description_localized("en-US", "Restore members' original (pre-bot) nicknames")
)]
pub async fn restore(
    ctx: Context<'_>,
    #[description = "Restore only this user (omit to restore everyone)"] user: Option<
        serenity::User,
    >,
) -> Result<(), Error> {
    if !require_manage_nicknames(ctx).await? {
        return Ok(());
    }
    ctx.defer().await?;

    let guild_id = ctx.guild_id().unwrap();
    let http = ctx.serenity_context().http.clone();

    let targets =
        crate::db::original_nicks(&ctx.data().db, guild_id, user.as_ref().map(|u| u.id.get()))
            .await?;

    if targets.is_empty() {
        ctx.say("Nothing to restore — no recorded nickname history for that selection.")
            .await?;
        return Ok(());
    }

    let total = targets.len();
    let channel_id = ctx.channel_id();

    ctx.say(format!(
        "↩️ Restoring original nickname(s) for **{total}** member(s) — this may take a moment…"
    ))
    .await?;

    tokio::spawn(async move {
        let mut restored = 0u32;
        let mut errors = 0u32;

        for (uid, old_nick) in &targets {
            // An empty string tells Discord to clear the nickname, which is the
            // correct behaviour when the user had none before the bot acted.
            let target = old_nick.as_deref().map_or("", truncate_nick);
            match guild_id
                .edit_member(
                    &http,
                    serenity::UserId::new(*uid),
                    serenity::EditMember::new().nickname(target),
                )
                .await
            {
                Ok(_) => restored += 1,
                Err(e) => {
                    tracing::warn!(
                        "Could not restore nick for {} in {}: {:?}",
                        uid,
                        guild_id,
                        e
                    );
                    errors += 1;
                }
            }
        }

        tracing::info!(guild = %guild_id, restored, errors, "Background restore task completed");
        let summary = format!(
            "✅ Restore complete! Restored: **{restored}** | ❌ Skipped/errors: **{errors}**"
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
            tracing::warn!(
                "Failed to send restore summary to channel {}: {:?}",
                channel_id,
                e
            );
        }
    });

    Ok(())
}
