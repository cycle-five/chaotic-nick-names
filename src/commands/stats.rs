use crate::{Context, Error};

/// Show nickname-change statistics for this server.
#[poise::command(
    slash_command,
    guild_only,
    description_localized("en-US", "Show nickname-change statistics for this server")
)]
pub async fn stats(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let (total, bulk, mut top_cats) = {
        let data = ctx.data().read().await;
        match data.guild(guild_id) {
            None => {
                ctx.say("No statistics recorded yet — try `/randomize` first!")
                    .await?;
                return Ok(());
            }
            Some(gs) => {
                let mut cats: Vec<(String, u64)> = gs
                    .stats
                    .category_usage
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                cats.sort_by(|a, b| b.1.cmp(&a.1));
                (gs.stats.total_changes, gs.stats.bulk_randomize_count, cats)
            }
        }
    };

    top_cats.truncate(5);

    let top_str = if top_cats.is_empty() {
        "*(none yet)*".to_string()
    } else {
        top_cats
            .iter()
            .enumerate()
            .map(|(i, (cat, count))| format!("{}. **{}** — {} change(s)", i + 1, cat, count))
            .collect::<Vec<_>>()
            .join("\n")
    };

    ctx.say(format!(
        "📊 **Nickname Statistics**\n\
         • Total nickname changes: **{total}**\n\
         • Bulk `/randomize` runs:  **{bulk}**\n\n\
         **Top categories:**\n{top_str}"
    ))
    .await?;

    Ok(())
}
