use std::time::Duration;

use poise::serenity_prelude::{self as serenity, futures::StreamExt};

use crate::db;
use crate::{Data, Error};

/// How far back to look for an assignment to attach feedback to. Older
/// nicknames are usually long-forgotten by the wearer, so feedback on them
/// is rarely useful and risks being noise for category curation.
const RECENT_DAYS: i32 = 30;

/// Maximum time the ephemeral feedback session waits for further clicks
/// before timing out. Discord's interaction token also limits this to 15
/// minutes, so we stay well under that.
const SESSION_TIMEOUT: Duration = Duration::from_secs(300);

/// Modal opened when the user clicks "Add note". Pre-filled with whatever
/// note the user typed earlier in the same session, if any.
#[derive(Debug, poise::Modal)]
#[name = "Feedback note"]
struct NoteModal {
    #[name = "Optional note (max 140 chars)"]
    #[placeholder = "What about this nickname surprised, annoyed, or delighted you?"]
    #[paragraph]
    #[min_length = 0]
    #[max_length = 140]
    note: Option<String>,
}

/// The "Is this nickname relevant to {category}?" tri-state. `Unset` is the
/// initial pre-interaction state and is treated as `NULL is_relevant` in the
/// DB; `Skip` is an explicit user-chosen "no opinion".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relevance {
    Unset,
    Yes,
    No,
    Skip,
}

impl Relevance {
    fn to_db_value(self) -> Option<bool> {
        match self {
            Relevance::Yes => Some(true),
            Relevance::No => Some(false),
            Relevance::Unset | Relevance::Skip => None,
        }
    }
}

/// Mutable feedback state that lives only on the stack for the duration of
/// the ephemeral session — discarded on submit, cancel, or timeout.
#[derive(Debug)]
struct FeedbackState {
    relevance: Relevance,
    nsfw_flag: bool,
    note: Option<String>,
}

/// Right-click yourself → Apps → **Give feedback on nickname**. Captures
/// per-nickname signal (relevance to category, NSFW miscategorization,
/// optional free-text note) and stores it in the `feedback` table keyed by
/// the originating `nick_changes` row.
#[poise::command(context_menu_command = "Give feedback on nickname", guild_only)]
pub async fn give_feedback(
    ctx: poise::ApplicationContext<'_, Data, Error>,
    user: serenity::User,
) -> Result<(), Error> {
    // Self-only for v1. Right-clicking someone else is rejected with a clear
    // ephemeral message; future versions may relax this for moderators.
    if user.id != ctx.interaction.user.id {
        ctx.send(
            poise::CreateReply::default()
                .content("You can only give feedback on your own nickname (for now).")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().ok_or("guild_only command outside guild")?;

    // The DB has the full history; the in-memory deque is capped at 200 so
    // a bulk randomize on a large guild can evict assignments we still want
    // to surface here. Query directly.
    let nc = match db::find_recent_nick_change(&ctx.data.db, guild_id, user.id.get(), RECENT_DAYS)
        .await?
    {
        Some(row) => row,
        None => {
            ctx.send(
                poise::CreateReply::default()
                    .content(format!(
                        "I haven't assigned you a nickname in the last {RECENT_DAYS} days. \
                         Try `/assign_random_nick` first, then come back to give feedback."
                    ))
                    .ephemeral(true),
            )
            .await?;
            return Ok(());
        }
    };

    // Scope every custom_id to this command invocation so other in-flight
    // feedback sessions don't catch each other's button clicks.
    let invocation_id = ctx.interaction.id;
    let id_relevance = format!("fb-rel-{invocation_id}");
    let id_nsfw = format!("fb-nsfw-{invocation_id}");
    let id_note = format!("fb-note-{invocation_id}");
    let id_submit = format!("fb-submit-{invocation_id}");
    let id_cancel = format!("fb-cancel-{invocation_id}");

    let mut state = FeedbackState {
        relevance: Relevance::Unset,
        nsfw_flag: false,
        note: None,
    };

    // Each render closure returns an owned builder so the inner FeedbackView
    // (which borrows both the captured refs and the per-call state ref)
    // doesn't leak its lifetimes into the caller. Two closures rather than
    // one because `to_reply()` and `to_response_message()` produce distinct
    // builder types.
    let render_reply = |s: &FeedbackState| {
        FeedbackView {
            nc: &nc,
            state: s,
            id_relevance: &id_relevance,
            id_nsfw: &id_nsfw,
            id_note: &id_note,
            id_submit: &id_submit,
            id_cancel: &id_cancel,
        }
        .to_reply()
    };
    let render_response = |s: &FeedbackState| {
        FeedbackView {
            nc: &nc,
            state: s,
            id_relevance: &id_relevance,
            id_nsfw: &id_nsfw,
            id_note: &id_note,
            id_submit: &id_submit,
            id_cancel: &id_cancel,
        }
        .to_response_message()
    };

    // Initial ephemeral render.
    let prompt = ctx.send(render_reply(&state).ephemeral(true)).await?;
    let msg_id = prompt.message().await?.id;
    let parent_ctx: crate::Context<'_> = poise::Context::Application(ctx);

    // Stream every component interaction on this message until the user
    // submits, cancels, or the session times out.
    let mut stream = serenity::ComponentInteractionCollector::new(ctx.serenity_context())
        .author_id(user.id)
        .message_id(msg_id)
        .timeout(SESSION_TIMEOUT)
        .stream();

    while let Some(mci) = stream.next().await {
        let cid = mci.data.custom_id.as_str();

        if cid == id_relevance {
            if let serenity::ComponentInteractionDataKind::StringSelect { values } = &mci.data.kind
            {
                state.relevance = match values.first().map(String::as_str) {
                    Some("yes") => Relevance::Yes,
                    Some("no") => Relevance::No,
                    Some("skip") => Relevance::Skip,
                    _ => Relevance::Unset,
                };
            }
        } else if cid == id_nsfw {
            state.nsfw_flag = !state.nsfw_flag;
        } else if cid == id_note {
            // Open a modal pre-filled with the existing note. The modal
            // submission auto-acks; we then have to repaint the original
            // ephemeral message ourselves via the reply handle.
            let prefilled = NoteModal {
                note: state.note.clone(),
            };
            let result = poise::execute_modal_on_component_interaction(
                ctx,
                mci,
                Some(prefilled),
                Some(Duration::from_secs(120)),
            )
            .await?;
            if let Some(NoteModal { note }) = result {
                state.note = note
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                prompt
                    .edit(parent_ctx, render_reply(&state).ephemeral(true))
                    .await?;
            }
            // If the modal was dismissed (result == None), leave state alone.
            continue;
        } else if cid == id_submit {
            db::upsert_feedback(
                &ctx.data.db,
                nc.id,
                user.id.get(),
                state.relevance.to_db_value(),
                state.nsfw_flag,
                state.note.as_deref(),
            )
            .await?;
            tracing::info!(
                "feedback recorded: nick_change={} user={} relevant={:?} nsfw_flag={} note_len={}",
                nc.id,
                user.id.get(),
                state.relevance,
                state.nsfw_flag,
                state.note.as_deref().map(str::len).unwrap_or(0),
            );
            mci.create_response(
                ctx.serenity_context(),
                serenity::CreateInteractionResponse::UpdateMessage(
                    serenity::CreateInteractionResponseMessage::new()
                        .content(format!(
                            "✅ Feedback recorded for **{}** (*{}*). Thanks!",
                            nc.new_nick, nc.category
                        ))
                        .components(vec![]),
                ),
            )
            .await?;
            return Ok(());
        } else if cid == id_cancel {
            mci.create_response(
                ctx.serenity_context(),
                serenity::CreateInteractionResponse::UpdateMessage(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Feedback cancelled — nothing was saved.")
                        .components(vec![]),
                ),
            )
            .await?;
            return Ok(());
        } else {
            // Unknown custom_id (shouldn't fire with invocation-scoped IDs).
            continue;
        }

        // Reached by id_relevance and id_nsfw branches only — the others
        // diverged via `continue` or `return` after handling their own
        // response. Repaint the prompt with the just-mutated state.
        mci.create_response(
            ctx.serenity_context(),
            serenity::CreateInteractionResponse::UpdateMessage(render_response(&state)),
        )
        .await?;
    }

    // Stream ended without an explicit submit / cancel ⇒ session timed out.
    // best-effort edit; ignore failure (interaction token may be expired).
    let _ = prompt
        .edit(
            parent_ctx,
            poise::CreateReply::default()
                .content(format!(
                    "⏱️ Feedback session timed out after {} minutes — no feedback saved.",
                    SESSION_TIMEOUT.as_secs() / 60
                ))
                .components(vec![])
                .ephemeral(true),
        )
        .await;

    Ok(())
}

// ── View helpers ─────────────────────────────────────────────────────────────

/// Render the message body shared by the initial send and every modal-driven
/// re-render. Returns the text only; components are added by the caller.
fn body_text(nc: &db::RecentNickChange, state: &FeedbackState) -> String {
    let when = nc.changed_at.format("%Y-%m-%d %H:%M UTC");
    let mut s = format!(
        "**Feedback on:** `{}` *({})*  · changed {}\n\n",
        nc.new_nick, nc.category, when
    );
    s.push_str(match state.relevance {
        Relevance::Unset => "• Relevance: *(not selected)*\n",
        Relevance::Yes => "• Relevance: ✅ relevant\n",
        Relevance::No => "• Relevance: ❌ not relevant\n",
        Relevance::Skip => "• Relevance: ⏭️ skipped\n",
    });
    s.push_str(&format!(
        "• NSFW miscategorized flag: {}\n",
        if state.nsfw_flag { "🔞 yes" } else { "—" }
    ));
    match state.note.as_deref() {
        // The modal enforces max_length=140 server-side, so anything we see
        // here already fits — no manual truncation needed.
        None | Some("") => s.push_str("• Note: *(none)*\n"),
        Some(note) => s.push_str(&format!("• Note: {note}\n")),
    }
    s
}

/// Build all four component rows. The StringSelect's `default_selection`
/// reflects current state so the UI matches what the user already chose;
/// the NSFW button label/style and the Note button label flip with state.
fn components(
    state: &FeedbackState,
    id_relevance: &str,
    id_nsfw: &str,
    id_note: &str,
    id_submit: &str,
    id_cancel: &str,
) -> Vec<serenity::CreateActionRow> {
    let opt = |value: &str, label: &str, this: Relevance| {
        serenity::CreateSelectMenuOption::new(label, value).default_selection(state.relevance == this)
    };
    let select = serenity::CreateSelectMenu::new(
        id_relevance,
        serenity::CreateSelectMenuKind::String {
            options: vec![
                opt("yes", "Yes — relevant to the category", Relevance::Yes),
                opt("no", "No — not relevant", Relevance::No),
                opt("skip", "Skip / no opinion", Relevance::Skip),
            ],
        },
    )
    .placeholder("Is this nickname relevant to the category?")
    .min_values(1)
    .max_values(1);

    let nsfw_btn = serenity::CreateButton::new(id_nsfw)
        .label(if state.nsfw_flag {
            "🔞 Flagged as NSFW ✓"
        } else {
            "🔞 Flag as NSFW miscategorized"
        })
        .style(if state.nsfw_flag {
            serenity::ButtonStyle::Danger
        } else {
            serenity::ButtonStyle::Secondary
        });
    let note_btn = serenity::CreateButton::new(id_note)
        .label(if state.note.is_some() {
            "📝 Edit note"
        } else {
            "📝 Add note (optional)"
        })
        .style(serenity::ButtonStyle::Secondary);
    let submit_btn = serenity::CreateButton::new(id_submit)
        .label("Submit")
        .style(serenity::ButtonStyle::Primary);
    let cancel_btn = serenity::CreateButton::new(id_cancel)
        .label("Cancel")
        .style(serenity::ButtonStyle::Secondary);

    vec![
        serenity::CreateActionRow::SelectMenu(select),
        serenity::CreateActionRow::Buttons(vec![nsfw_btn, note_btn]),
        serenity::CreateActionRow::Buttons(vec![submit_btn, cancel_btn]),
    ]
}

/// A bundle of references that together render the feedback prompt. Holds
/// just the inputs `body_text` and `components` need, so callers don't have
/// to thread seven arguments through every `build_*` site. Methods provide
/// the two builders (`poise::CreateReply` for the initial ephemeral send and
/// for `ReplyHandle::edit`, `CreateInteractionResponseMessage` for
/// component-interaction `UpdateMessage` responses).
struct FeedbackView<'a> {
    nc: &'a db::RecentNickChange,
    state: &'a FeedbackState,
    id_relevance: &'a str,
    id_nsfw: &'a str,
    id_note: &'a str,
    id_submit: &'a str,
    id_cancel: &'a str,
}

impl FeedbackView<'_> {
    fn content(&self) -> String {
        body_text(self.nc, self.state)
    }

    fn rows(&self) -> Vec<serenity::CreateActionRow> {
        // Calls the free `components` fn — `self.rows()` would be ambiguous
        // with this method name otherwise.
        components(
            self.state,
            self.id_relevance,
            self.id_nsfw,
            self.id_note,
            self.id_submit,
            self.id_cancel,
        )
    }

    fn to_reply(&self) -> poise::CreateReply {
        poise::CreateReply::default()
            .content(self.content())
            .components(self.rows())
    }

    fn to_response_message(&self) -> serenity::CreateInteractionResponseMessage {
        serenity::CreateInteractionResponseMessage::new()
            .content(self.content())
            .components(self.rows())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_to_db_value_maps_correctly() {
        assert_eq!(Relevance::Unset.to_db_value(), None);
        assert_eq!(Relevance::Skip.to_db_value(), None);
        assert_eq!(Relevance::Yes.to_db_value(), Some(true));
        assert_eq!(Relevance::No.to_db_value(), Some(false));
    }

    #[test]
    fn body_text_reflects_state() {
        let nc = db::RecentNickChange {
            id: 42,
            category: "scientists".into(),
            new_nick: "Marie Curie".into(),
            changed_at: chrono::Utc::now(),
        };
        let state = FeedbackState {
            relevance: Relevance::Yes,
            nsfw_flag: true,
            note: Some("perfect match".into()),
        };
        let body = body_text(&nc, &state);
        assert!(body.contains("Marie Curie"));
        assert!(body.contains("scientists"));
        assert!(body.contains("✅ relevant"));
        assert!(body.contains("🔞 yes"));
        assert!(body.contains("perfect match"));
    }

    #[test]
    fn body_text_handles_empty_state() {
        let nc = db::RecentNickChange {
            id: 1,
            category: "spices".into(),
            new_nick: "Cumin".into(),
            changed_at: chrono::Utc::now(),
        };
        let state = FeedbackState {
            relevance: Relevance::Unset,
            nsfw_flag: false,
            note: None,
        };
        let body = body_text(&nc, &state);
        assert!(body.contains("*(not selected)*"));
        assert!(body.contains("Note: *(none)*"));
    }
}
