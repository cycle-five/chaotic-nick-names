use poise::serenity_prelude as serenity;

use crate::commands::perms::require_manage_guild;
use crate::{data, Context, Error};

/// Validate a category key: must be non-empty, start with a letter, and contain
/// only letters, digits, or underscores.
fn valid_category_key(key: &str) -> bool {
    !key.is_empty()
        && key.chars().next().is_some_and(char::is_alphabetic)
        && key.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Parse a CSV import body into `(categories, line_errors)`.
///
/// Each non-blank, non-`#` line is `category_name,name1,name2,…`.  Category
/// names are lower-cased; surrounding whitespace on every field is trimmed and
/// empty names are dropped.  This is pure (no I/O) so it can be unit-tested.
pub fn parse_category_csv(text: &str) -> (Vec<(String, Vec<String>)>, Vec<String>) {
    let mut added: Vec<(String, Vec<String>)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut fields = line.split(',');
        let raw_key = fields.next().unwrap_or("").trim();
        let key = raw_key.to_lowercase();

        if !valid_category_key(&key) {
            errors.push(format!(
                "Line {}: invalid category name `{}` (must start with a letter, alphanumeric and underscores only)",
                line_no + 1,
                raw_key
            ));
            continue;
        }

        let names: Vec<String> = fields
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if names.is_empty() {
            errors.push(format!(
                "Line {}: category `{}` has no names",
                line_no + 1,
                key
            ));
            continue;
        }

        added.push((key, names));
    }

    (added, errors)
}

// ── /categories (parent with four sub-commands) ──────────────────────────────

/// Manage and view the nickname categories available in this server.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("list", "add", "remove", "import"),
    description_localized("en-US", "Manage nickname categories")
)]
pub async fn categories(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// List all available nickname categories.
#[poise::command(
    slash_command,
    guild_only,
    rename = "list",
    description_localized("en-US", "List all available nickname categories")
)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let all_cats = {
        let data = ctx.data().read_state().await;
        match data.guild(guild_id) {
            Some(gs) => gs.all_categories(),
            None => data::builtin_categories(),
        }
    };

    let builtin_names = data::builtin_category_names();

    let format_line = |name: &String, items: &Vec<String>| {
        let tag = if builtin_names.contains(name) {
            ""
        } else {
            " *(custom)*"
        };
        format!("• **{}**{} — {} name(s)", name, tag, items.len())
    };

    let mut standard: Vec<String> = Vec::new();
    let mut nsfw: Vec<String> = Vec::new();
    for (name, items) in &all_cats {
        let line = format_line(name, items);
        if data::is_nsfw(name) {
            nsfw.push(line);
        } else {
            standard.push(line);
        }
    }
    standard.sort();
    nsfw.sort();

    let mut out = format!("**Available categories** ({} total)", all_cats.len());
    if !standard.is_empty() {
        out.push_str("\n**Standard**\n");
        out.push_str(&standard.join("\n"));
    }
    if !nsfw.is_empty() {
        out.push_str("\n\n**🔞 NSFW (18+)** — only chosen if requested explicitly\n");
        out.push_str(&nsfw.join("\n"));
    }
    ctx.say(out).await?;

    Ok(())
}

/// Add a new custom category with a comma-separated list of names.
///
/// Requires the **Manage Guild** permission.
#[poise::command(
    slash_command,
    guild_only,
    rename = "add",
    description_localized("en-US", "Add a custom nickname category")
)]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Category name (alphanumeric and underscores)"] name: String,
    #[description = "Comma-separated list of nickname values"] items: String,
) -> Result<(), Error> {
    if !require_manage_guild(ctx).await? {
        return Ok(());
    }
    // Validate name
    let key = name.to_lowercase();
    if !valid_category_key(&key) {
        ctx.say("❌ Category name must start with a letter and contain only letters, digits, or underscores.").await?;
        return Ok(());
    }

    let parsed: Vec<String> = items
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if parsed.is_empty() {
        ctx.say("❌ You must provide at least one name.").await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().unwrap();

    {
        let mut data = ctx.data().write_state().await;
        let gs = data.guild_mut(guild_id);
        gs.custom_categories.insert(key.clone(), parsed.clone());
        // Clear the used-names pool so the updated list starts fresh
        gs.reset_pool(Some(&key));
    }

    // Persist to DB (best-effort)
    {
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let k = key.clone();
        let p = parsed.clone();
        tokio::spawn(async move {
            let _ = crate::db::upsert_custom_category(&db, gid, &k, &p).await;
            let _ = crate::db::clear_used_names(&db, gid, Some(&k)).await;
        });
    }

    ctx.say(format!(
        "✅ Added custom category **{}** with **{}** name(s).",
        key,
        parsed.len()
    ))
    .await?;

    Ok(())
}

/// Remove a custom category. Built-in categories cannot be removed.
///
/// Requires the **Manage Guild** permission.
#[poise::command(
    slash_command,
    guild_only,
    rename = "remove",
    description_localized("en-US", "Remove a custom nickname category")
)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Name of the custom category to remove"] name: String,
) -> Result<(), Error> {
    if !require_manage_guild(ctx).await? {
        return Ok(());
    }
    let key = name.to_lowercase();
    let guild_id = ctx.guild_id().unwrap();

    // A custom category that *shadows* a built-in name is still removable —
    // removing it simply restores the built-in. Only reject when there is no
    // custom category of this name at all and the name belongs to a built-in.
    let removed = {
        let mut data = ctx.data().write_state().await;
        let gs = data.guild_mut(guild_id);
        gs.remove_custom_category(&key)
    };

    if !removed && data::builtin_category_names().contains(&key) {
        ctx.say("❌ Built-in categories cannot be removed.").await?;
        return Ok(());
    }

    if removed {
        // Persist to DB (best-effort)
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let k = key.clone();
        tokio::spawn(async move {
            let _ = crate::db::delete_custom_category(&db, gid, &k).await;
            let _ = crate::db::clear_used_names(&db, gid, Some(&k)).await;
        });

        ctx.say(format!("✅ Removed custom category **{key}**."))
            .await?;
    } else {
        ctx.say(format!("❌ No custom category named `{key}` found."))
            .await?;
    }

    Ok(())
}

/// Import categories from an attached CSV file.
///
/// Each row in the CSV must start with the **category name**, followed by the
/// names in that category:
/// ```
/// scientists,Einstein,Newton,Darwin,Curie
/// amusement_parks,Cedar Point,Dollywood,Alton Towers
/// ```
/// Multiple rows create or replace multiple categories at once.
/// Requires the **Manage Guild** permission.
#[poise::command(
    slash_command,
    guild_only,
    rename = "import",
    description_localized("en-US", "Import categories from a CSV file attachment")
)]
pub async fn import(
    ctx: Context<'_>,
    #[description = "CSV file (each row: category_name,name1,name2,…)"] file: serenity::Attachment,
) -> Result<(), Error> {
    if !require_manage_guild(ctx).await? {
        return Ok(());
    }
    ctx.defer().await?;

    // Guard against oversized uploads (1 MiB limit)
    if file.size > 1_048_576 {
        ctx.say("❌ File too large (max 1 MiB).").await?;
        return Ok(());
    }

    let bytes = file.download().await?;
    let text = String::from_utf8(bytes).map_err(|_| "❌ File is not valid UTF-8.")?;

    let guild_id = ctx.guild_id().unwrap();
    let (parsed, errors) = parse_category_csv(&text);

    let mut added: Vec<(String, usize)> = Vec::new();
    for (key, names) in parsed {
        let count = names.len();

        // Update in-memory state
        {
            let mut data = ctx.data().write_state().await;
            let gs = data.guild_mut(guild_id);
            gs.custom_categories.insert(key.clone(), names.clone());
            gs.reset_pool(Some(&key));
        }

        // Persist to DB (best-effort)
        let db = ctx.data().db.clone();
        let gid = guild_id;
        let k = key.clone();
        let n = names.clone();
        tokio::spawn(async move {
            let _ = crate::db::upsert_custom_category(&db, gid, &k, &n).await;
            let _ = crate::db::clear_used_names(&db, gid, Some(&k)).await;
        });

        added.push((key, count));
    }

    let mut reply = String::new();
    if !added.is_empty() {
        let summary: Vec<String> = added
            .iter()
            .map(|(cat, n)| format!("• **{cat}** ({n} name(s))"))
            .collect();
        reply.push_str(&format!(
            "✅ Imported **{}** categor{}:\n{}",
            added.len(),
            if added.len() == 1 { "y" } else { "ies" },
            summary.join("\n")
        ));
    }
    if !errors.is_empty() {
        if !reply.is_empty() {
            reply.push('\n');
        }
        reply.push_str(&format!(
            "⚠️ **{}** line(s) skipped:\n{}",
            errors.len(),
            errors.join("\n")
        ));
    }
    if reply.is_empty() {
        reply = "❌ No valid categories found in the file.".to_string();
    }

    ctx.say(reply).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── valid_category_key ────────────────────────────────────────────────────

    #[test]
    fn valid_key_simple() {
        assert!(valid_category_key("scientists"));
        assert!(valid_category_key("my_cat_2"));
        assert!(valid_category_key("a"));
    }

    #[test]
    fn invalid_key_empty() {
        assert!(!valid_category_key(""));
    }

    #[test]
    fn invalid_key_starts_with_digit_or_underscore() {
        assert!(!valid_category_key("2cool"));
        assert!(!valid_category_key("_hidden"));
    }

    #[test]
    fn invalid_key_with_spaces_or_punctuation() {
        assert!(!valid_category_key("my cat"));
        assert!(!valid_category_key("cat-name"));
        assert!(!valid_category_key("cat!"));
    }

    // ── parse_category_csv ────────────────────────────────────────────────────

    #[test]
    fn csv_parses_basic_rows() {
        let (cats, errs) = parse_category_csv("scientists,Einstein,Newton\nplanets,Mars,Venus");
        assert!(errs.is_empty());
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[0].0, "scientists");
        assert_eq!(cats[0].1, vec!["Einstein", "Newton"]);
        assert_eq!(cats[1].0, "planets");
    }

    #[test]
    fn csv_lowercases_keys_and_trims_fields() {
        let (cats, _) = parse_category_csv("  Heroes , Batman ,  Robin ");
        assert_eq!(cats[0].0, "heroes");
        assert_eq!(cats[0].1, vec!["Batman", "Robin"]);
    }

    #[test]
    fn csv_skips_comments_and_blanks() {
        let (cats, errs) = parse_category_csv("# a comment\n\n   \nheroes,Batman");
        assert!(errs.is_empty());
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].0, "heroes");
    }

    #[test]
    fn csv_reports_invalid_key() {
        let (cats, errs) = parse_category_csv("2bad,Foo,Bar");
        assert!(cats.is_empty());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("Line 1"));
    }

    #[test]
    fn csv_reports_category_with_no_names() {
        let (cats, errs) = parse_category_csv("empty,,, ,");
        assert!(cats.is_empty());
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("no names"));
    }

    #[test]
    fn csv_handles_crlf_line_endings() {
        let (cats, errs) = parse_category_csv("heroes,Batman\r\nvillains,Joker\r\n");
        assert!(errs.is_empty());
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[1].1, vec!["Joker"]);
    }

    #[test]
    fn csv_mixed_valid_and_invalid_lines() {
        let (cats, errs) = parse_category_csv("good,A,B\n3bad,C\nalso_good,D");
        assert_eq!(cats.len(), 2);
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn csv_empty_input_yields_nothing() {
        let (cats, errs) = parse_category_csv("");
        assert!(cats.is_empty());
        assert!(errs.is_empty());
    }
}
