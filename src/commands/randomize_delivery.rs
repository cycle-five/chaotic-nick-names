//! Resilient delivery of the background-randomize summary.
//!
//! A bulk `/randomize` applies nick edits from a detached `tokio::spawn` that
//! can outlive the slash interaction, so the final summary has to be delivered
//! from outside the original command context. The historic approach — a raw
//! channel POST — fails (HTTP 403 `Missing Access`) whenever the bot lacks
//! View/Send in the channel the command was invoked from, even though Discord
//! happily delivered the interaction itself. This module routes the summary
//! through surfaces that don't depend on channel permissions, falling back in
//! order: edit the interaction response → DM the invoker → dead-letter it.

use poise::serenity_prelude as serenity;
use sqlx::PgPool;

/// Which surface actually carried the summary to the user. Returned for logging
/// and so callers can reason about (and tests can assert) the fallback outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// Edited the original slash-command response (the common, best case).
    Interaction,
    /// Fell back to a direct message to the invoker.
    DirectMessage,
    /// Parked in the dead-letter table for the user's next `/randomize`.
    DeadLettered,
    /// Every path failed, including the database — only logged.
    Lost,
}

/// Deliver `summary` to the user who ran `/randomize`, trying the surfaces that
/// don't depend on the bot's channel permissions, in order. Never returns an
/// error: the worst case is [`DeliveryOutcome::Lost`], which is logged.
pub async fn deliver_summary(
    http: &serenity::Http,
    db: &PgPool,
    token: Option<&str>,
    progress_msg_id: serenity::MessageId,
    invoker_id: serenity::UserId,
    guild_id: serenity::GuildId,
    summary: &str,
) -> DeliveryOutcome {
    // 1. Edit the "Randomizing…" progress message in place. After `defer()` that
    //    message is an interaction *followup* (not the `@original` deferred
    //    placeholder), so we edit it by id through the interaction webhook —
    //    Discord authorises this by token, so the bot needing View/Send in the
    //    invoking channel is irrelevant, which is exactly the failure we are
    //    fixing. Only works while the token is live (~15 min from invocation).
    if let Some(token) = token {
        let builder = serenity::CreateInteractionResponseFollowup::new()
            .content(summary)
            .allowed_mentions(serenity::CreateAllowedMentions::new());
        match http
            .edit_followup_message(token, progress_msg_id, &builder, Vec::new())
            .await
        {
            Ok(_) => return DeliveryOutcome::Interaction,
            Err(e) => tracing::debug!(
                "randomize summary: followup edit failed (token likely expired): {e:?}"
            ),
        }
    }

    // 2. DM the invoker. Fails if they have DMs closed (HTTP 403, code 50007).
    match invoker_id.create_dm_channel(http).await {
        Ok(dm) => {
            let msg = serenity::CreateMessage::new()
                .content(summary)
                .allowed_mentions(serenity::CreateAllowedMentions::new());
            match dm.id.send_message(http, msg).await {
                Ok(_) => return DeliveryOutcome::DirectMessage,
                Err(e) => tracing::debug!("randomize summary: DM send failed: {e:?}"),
            }
        }
        Err(e) => tracing::debug!("randomize summary: opening DM channel failed: {e:?}"),
    }

    // 3. Dead-letter it, to be surfaced on the user's next /randomize.
    match crate::db::insert_undelivered_summary(db, guild_id, invoker_id.get(), summary).await {
        Ok(()) => DeliveryOutcome::DeadLettered,
        Err(e) => {
            tracing::warn!("randomize summary could not be delivered or persisted (lost): {e:?}");
            DeliveryOutcome::Lost
        }
    }
}

/// The user-facing summary line for a completed bulk randomize.
pub fn summary_text(changed: u32, errors: u32) -> String {
    format!("✅ Randomization complete! Changed: **{changed}** | ❌ Skipped/errors: **{errors}**")
}

/// Render any summaries that were dead-lettered earlier (because we couldn't
/// reach the user at the time) into a single message, surfaced the next time the
/// user interacts. Empty in → empty out, so callers never post a bare header.
pub fn render_recovered_summaries(summaries: &[String]) -> String {
    if summaries.is_empty() {
        return String::new();
    }
    let mut out =
        String::from("📬 Results from an earlier randomize run I couldn't deliver at the time:\n");
    for s in summaries {
        out.push_str("\n• ");
        out.push_str(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_text_reports_changed_and_error_counts() {
        let s = summary_text(303, 2);
        assert!(s.contains("**303**"), "should show the changed count: {s}");
        assert!(s.contains("**2**"), "should show the error count: {s}");
        assert!(
            s.to_lowercase().contains("complete"),
            "should read as a completion message: {s}"
        );
    }

    #[test]
    fn summary_text_handles_zero_errors() {
        let s = summary_text(10, 0);
        assert!(s.contains("**10**"));
        assert!(s.contains("**0**"));
    }

    #[test]
    fn render_recovered_lists_each_summary_under_a_header() {
        let out = render_recovered_summaries(&[
            "First run done".to_string(),
            "Second run done".to_string(),
        ]);
        assert!(
            out.contains("First run done"),
            "must include each summary: {out}"
        );
        assert!(
            out.contains("Second run done"),
            "must include each summary: {out}"
        );
        assert!(
            out.to_lowercase().contains("earlier"),
            "needs a header explaining these are delayed/earlier results: {out}"
        );
    }

    #[test]
    fn render_recovered_empty_is_empty_string() {
        // The flush path only calls this when there is something to show, but a
        // defensive empty-in / empty-out keeps callers from posting a bare header.
        assert_eq!(render_recovered_summaries(&[]), "");
    }
}
