use crate::{Context, Error};

/// Show the recent nickname-change history for this server.
#[poise::command(
    slash_command,
    guild_only,
    description_localized("en-US", "Show recent nickname changes in this server")
)]
pub async fn history(
    ctx: Context<'_>,
    #[description = "Number of entries to show (1–25, default 10)"] limit: Option<u8>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let limit = limit.unwrap_or(10).clamp(1, 25) as usize;

    let entries = {
        let data = ctx.data().read().await;
        match data.guild(guild_id) {
            None => {
                ctx.say("No history recorded yet — try `/randomize` first!")
                    .await?;
                return Ok(());
            }
            Some(gs) => gs
                .history
                .iter()
                .take(limit)
                .cloned()
                .collect::<Vec<_>>(),
        }
    };

    if entries.is_empty() {
        ctx.say("No history recorded yet.").await?;
        return Ok(());
    }

    let lines: Vec<String> = entries
        .iter()
        .map(|e| {
            let old = e
                .old_nick
                .as_deref()
                .unwrap_or(&e.user_name);
            format!(
                "`{}` **{}** → **{}** *({})*",
                e.timestamp.format("%Y-%m-%d %H:%M UTC"),
                old,
                e.new_nick,
                e.category
            )
        })
        .collect();

    ctx.say(format!(
        "📜 **Recent nickname changes** (last {})\n{}",
        entries.len(),
        lines.join("\n")
    ))
    .await?;

    Ok(())
}
