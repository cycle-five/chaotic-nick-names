use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};
use poise::serenity_prelude::GuildId;

use crate::data;

pub struct AppState {
    guilds: HashMap<GuildId, GuildState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            guilds: HashMap::new(),
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

    /// Reset the used-name pool for `category` (or every category if `None`).
    pub fn reset_pool(&mut self, category: Option<&str>) {
        match category {
            Some(cat) => {
                self.used_names.remove(cat);
            }
            None => self.used_names.clear(),
        }
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
