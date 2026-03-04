use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use poise::serenity_prelude::GuildId;

use crate::data;

pub struct AppState {
    pub guilds: HashMap<GuildId, GuildState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            guilds: HashMap::new(),
        }
    }

    /// Build an `AppState` pre-populated from database rows (used at startup).
    pub fn from_guilds(guilds: Vec<(GuildId, GuildState)>) -> Self {
        Self {
            guilds: guilds.into_iter().collect(),
        }
    }

    pub fn guild(&self, guild_id: GuildId) -> Option<&GuildState> {
        self.guilds.get(&guild_id)
    }

    pub fn guild_mut(&mut self, guild_id: GuildId) -> &mut GuildState {
        self.guilds.entry(guild_id).or_insert_with(GuildState::new)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct GuildState {
    /// User-added categories (built-in categories live in data.rs)
    pub custom_categories: HashMap<String, Vec<String>>,
    /// Names already handed out per category (for without-replacement draws)
    pub used_names: HashMap<String, HashSet<String>>,
    /// Recent nickname-change history (newest first, capped at 200)
    pub history: VecDeque<HistoryEntry>,
    /// Aggregate stats for this guild
    pub stats: GuildStats,
}

impl GuildState {
    pub fn new() -> Self {
        Self {
            custom_categories: HashMap::new(),
            used_names: HashMap::new(),
            history: VecDeque::new(),
            stats: GuildStats::default(),
        }
    }

    /// Return all available categories: built-in merged with custom.
    /// Custom categories with the same key override built-ins.
    pub fn all_categories(&self) -> HashMap<String, Vec<String>> {
        let mut cats = data::builtin_categories();
        for (k, v) in &self.custom_categories {
            cats.insert(k.clone(), v.clone());
        }
        cats
    }

    /// Pick one name from `category` using without-replacement semantics.
    /// When the pool for a category is exhausted it resets automatically.
    pub fn pick_name(&mut self, category: &str, names: &[String]) -> Option<String> {
        if names.is_empty() {
            return None;
        }

        let used = self.used_names.entry(category.to_string()).or_default();

        let mut available: Vec<&String> = names.iter().filter(|n| !used.contains(*n)).collect();

        if available.is_empty() {
            // Pool exhausted – reset and start over
            used.clear();
            available = names.iter().collect();
        }

        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let picked = available.choose(&mut rng)?.to_string();

        self.used_names
            .entry(category.to_string())
            .or_default()
            .insert(picked.clone());

        Some(picked)
    }

    /// Use a specific name from `category`, marking it as used.
    /// Returns an error string if the name is not in `all_names`.
    pub fn use_specific_name(&mut self, category: &str, name: &str, all_names: &[String]) -> Result<String, String> {
        if !all_names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return Err(format!(
                "**{}** is not in the **{}** category.",
                name, category
            ));
        }
        // Find the canonical casing
        let canonical = all_names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(name))
            .cloned()
            .unwrap();
        self.used_names
            .entry(category.to_string())
            .or_default()
            .insert(canonical.clone());
        Ok(canonical)
    }

    /// Reset the used-name pool for `category` (or every category if `None`).
    pub fn reset_pool(&mut self, category: Option<&str>) {
        match category {
            Some(cat) => {
                self.used_names.remove(cat);
            }
            None => self.used_names.clear(),
        }
    }

    /// Remove a custom category and clean up all associated tracking data
    /// (`used_names` and `stats.category_usage`).
    /// Returns `true` if the category existed and was removed.
    pub fn remove_custom_category(&mut self, key: &str) -> bool {
        let removed = self.custom_categories.remove(key).is_some();
        if removed {
            self.used_names.remove(key);
            self.stats.category_usage.remove(key);
        }
        removed
    }

    /// Prepend a history entry, keeping the deque at most 200 entries.
    pub fn add_history(&mut self, entry: HistoryEntry) {
        self.history.push_front(entry);
        self.history.truncate(200);
    }

    /// Record a successful nickname change in both history and stats.
    pub fn record_change(
        &mut self,
        user_id: u64,
        user_name: String,
        old_nick: Option<String>,
        new_nick: String,
        category: String,
    ) {
        self.add_history(HistoryEntry {
            timestamp: Utc::now(),
            user_id,
            user_name,
            old_nick,
            new_nick: new_nick.clone(),
            category: category.clone(),
        });
        self.stats.total_changes += 1;
        *self.stats.category_usage.entry(category).or_insert(0) += 1;
    }
}

impl Default for GuildState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub user_id: u64,
    pub user_name: String,
    pub old_nick: Option<String>,
    pub new_nick: String,
    pub category: String,
}

#[derive(Clone, Default)]
pub struct GuildStats {
    pub total_changes: u64,
    pub category_usage: HashMap<String, u64>,
    pub bulk_randomize_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // ── GuildState::pick_name ─────────────────────────────────────────────────

    #[test]
    fn pick_name_returns_name_from_list() {
        let mut gs = GuildState::new();
        let list = names(&["Alpha", "Beta", "Gamma"]);
        let picked = gs.pick_name("test", &list).unwrap();
        assert!(list.contains(&picked));
    }

    #[test]
    fn pick_name_empty_list_returns_none() {
        let mut gs = GuildState::new();
        assert!(gs.pick_name("test", &[]).is_none());
    }

    #[test]
    fn pick_name_without_replacement_no_repeats_until_exhausted() {
        let mut gs = GuildState::new();
        let list = names(&["A", "B", "C"]);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..list.len() {
            let picked = gs.pick_name("test", &list).unwrap();
            assert!(!seen.contains(&picked), "duplicate pick before pool exhausted");
            seen.insert(picked);
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn pick_name_resets_pool_after_exhaustion() {
        let mut gs = GuildState::new();
        let list = names(&["X", "Y"]);
        gs.pick_name("test", &list);
        gs.pick_name("test", &list);
        // Next pick should succeed (pool was auto-reset)
        assert!(gs.pick_name("test", &list).is_some());
    }

    // ── GuildState::use_specific_name ─────────────────────────────────────────

    #[test]
    fn use_specific_name_valid_name_returns_canonical() {
        let mut gs = GuildState::new();
        let list = names(&["Einstein", "Newton"]);
        let result = gs.use_specific_name("scientists", "einstein", &list);
        assert_eq!(result.unwrap(), "Einstein");
    }

    #[test]
    fn use_specific_name_invalid_name_returns_error() {
        let mut gs = GuildState::new();
        let list = names(&["Einstein", "Newton"]);
        let result = gs.use_specific_name("scientists", "Darwin", &list);
        assert!(result.is_err());
    }

    #[test]
    fn use_specific_name_marks_as_used() {
        let mut gs = GuildState::new();
        let list = names(&["A", "B"]);
        gs.use_specific_name("cat", "A", &list).unwrap();
        assert!(gs.used_names["cat"].contains("A"));
    }

    // ── GuildState::reset_pool ────────────────────────────────────────────────

    #[test]
    fn reset_pool_specific_category_clears_it() {
        let mut gs = GuildState::new();
        let list = names(&["A", "B"]);
        gs.pick_name("cat", &list);
        gs.reset_pool(Some("cat"));
        assert!(!gs.used_names.contains_key("cat"));
    }

    #[test]
    fn reset_pool_none_clears_all() {
        let mut gs = GuildState::new();
        let list = names(&["A"]);
        gs.pick_name("cat1", &list);
        gs.pick_name("cat2", &list);
        gs.reset_pool(None);
        assert!(gs.used_names.is_empty());
    }

    // ── GuildState::remove_custom_category ────────────────────────────────────

    #[test]
    fn remove_custom_category_returns_true_when_present() {
        let mut gs = GuildState::new();
        gs.custom_categories.insert("mycat".to_string(), vec!["A".to_string()]);
        assert!(gs.remove_custom_category("mycat"));
    }

    #[test]
    fn remove_custom_category_returns_false_when_absent() {
        let mut gs = GuildState::new();
        assert!(!gs.remove_custom_category("nonexistent"));
    }

    #[test]
    fn remove_custom_category_cleans_up_used_names() {
        let mut gs = GuildState::new();
        let list = names(&["A", "B"]);
        gs.custom_categories.insert("mycat".to_string(), list.clone());
        gs.pick_name("mycat", &list);
        assert!(gs.used_names.contains_key("mycat"));
        gs.remove_custom_category("mycat");
        assert!(!gs.used_names.contains_key("mycat"));
    }

    #[test]
    fn remove_custom_category_cleans_up_stats() {
        let mut gs = GuildState::new();
        gs.custom_categories.insert("mycat".to_string(), vec!["A".to_string()]);
        gs.record_change(1, "user".to_string(), None, "A".to_string(), "mycat".to_string());
        assert!(gs.stats.category_usage.contains_key("mycat"));
        gs.remove_custom_category("mycat");
        assert!(!gs.stats.category_usage.contains_key("mycat"));
    }

    #[test]
    fn remove_custom_category_does_not_affect_other_categories() {
        let mut gs = GuildState::new();
        let list = names(&["X"]);
        gs.custom_categories.insert("cat_a".to_string(), list.clone());
        gs.custom_categories.insert("cat_b".to_string(), list.clone());
        gs.pick_name("cat_b", &list);
        gs.remove_custom_category("cat_a");
        assert!(gs.custom_categories.contains_key("cat_b"));
        assert!(gs.used_names.contains_key("cat_b"));
    }

    // ── GuildState::all_categories ────────────────────────────────────────────

    #[test]
    fn all_categories_includes_builtins() {
        let gs = GuildState::new();
        let cats = gs.all_categories();
        assert!(cats.contains_key("scientists"));
        assert!(cats.contains_key("planets"));
    }

    #[test]
    fn all_categories_custom_overrides_builtin() {
        let mut gs = GuildState::new();
        gs.custom_categories
            .insert("scientists".to_string(), vec!["CustomGuy".to_string()]);
        let cats = gs.all_categories();
        assert_eq!(cats["scientists"], vec!["CustomGuy".to_string()]);
    }

    #[test]
    fn all_categories_includes_extra_custom() {
        let mut gs = GuildState::new();
        gs.custom_categories
            .insert("my_custom".to_string(), vec!["FooBar".to_string()]);
        let cats = gs.all_categories();
        assert!(cats.contains_key("my_custom"));
    }

    // ── GuildState::add_history / record_change ───────────────────────────────

    #[test]
    fn add_history_prepends_entry() {
        let mut gs = GuildState::new();
        gs.add_history(HistoryEntry {
            timestamp: Utc::now(),
            user_id: 1,
            user_name: "alice".to_string(),
            old_nick: None,
            new_nick: "Newton".to_string(),
            category: "scientists".to_string(),
        });
        assert_eq!(gs.history.len(), 1);
        assert_eq!(gs.history[0].new_nick, "Newton");
    }

    #[test]
    fn add_history_capped_at_200() {
        let mut gs = GuildState::new();
        for i in 0..210u64 {
            gs.add_history(HistoryEntry {
                timestamp: Utc::now(),
                user_id: i,
                user_name: "user".to_string(),
                old_nick: None,
                new_nick: format!("nick{}", i),
                category: "test".to_string(),
            });
        }
        assert_eq!(gs.history.len(), 200);
    }

    #[test]
    fn record_change_increments_total_changes() {
        let mut gs = GuildState::new();
        gs.record_change(1, "alice".to_string(), None, "Newton".to_string(), "scientists".to_string());
        assert_eq!(gs.stats.total_changes, 1);
    }

    #[test]
    fn record_change_increments_category_usage() {
        let mut gs = GuildState::new();
        gs.record_change(1, "alice".to_string(), None, "Newton".to_string(), "scientists".to_string());
        gs.record_change(2, "bob".to_string(), None, "Einstein".to_string(), "scientists".to_string());
        assert_eq!(gs.stats.category_usage["scientists"], 2);
    }

    // ── AppState ──────────────────────────────────────────────────────────────

    #[test]
    fn app_state_from_guilds_loads_correctly() {
        use poise::serenity_prelude::GuildId;
        let gid = GuildId::new(42);
        let gs = GuildState::new();
        let state = AppState::from_guilds(vec![(gid, gs)]);
        assert!(state.guild(gid).is_some());
    }

    #[test]
    fn app_state_guild_mut_creates_default_on_missing() {
        use poise::serenity_prelude::GuildId;
        let mut state = AppState::new();
        let gid = GuildId::new(99);
        let gs = state.guild_mut(gid);
        assert!(gs.history.is_empty());
    }
}
