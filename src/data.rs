use std::collections::HashMap;
use std::sync::OnceLock;

/// Built-in name categories, embedded from `categories.json` at compile time.
/// Parsing happens once and the result is cached for the lifetime of the process.
const CATEGORIES_JSON: &str = include_str!("categories.json");

fn categories() -> &'static HashMap<String, Vec<String>> {
    static CACHE: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        serde_json::from_str(CATEGORIES_JSON)
            .expect("src/categories.json must be valid JSON of {category: [names]}")
    })
}

/// Return a fresh copy of the built-in name categories.
pub fn builtin_categories() -> HashMap<String, Vec<String>> {
    categories().clone()
}

/// Built-in category names as a sorted list (for display).
pub fn builtin_category_names() -> Vec<String> {
    let mut names: Vec<String> = categories().keys().cloned().collect();
    names.sort();
    names
}

/// Category names that are NSFW / 18+. They remain in the catalog and can be
/// invoked explicitly (e.g. `/randomize category:serial_killers`) but are
/// excluded from random-pool selection by default — see
/// `randomize::pick_random_category`. Listed names MUST exist as real
/// categories in `categories.json` (enforced by a test below); add an entry
/// here in the same PR that adds the category's data.
pub const NSFW: &[&str] = &[];

/// Case-insensitive membership test against [`NSFW`].
pub fn is_nsfw(name: &str) -> bool {
    NSFW.iter().any(|n| n.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_categories_is_non_empty() {
        let cats = builtin_categories();
        assert!(!cats.is_empty());
    }

    #[test]
    fn nsfw_names_are_real_categories() {
        let cats = builtin_categories();
        for name in NSFW {
            assert!(
                cats.contains_key(*name),
                "NSFW entry '{}' is not a real category in categories.json",
                name
            );
        }
    }

    #[test]
    fn is_nsfw_is_case_insensitive() {
        // Vacuously true while NSFW is empty; exercises the predicate so any
        // future addition is regression-tested for casing.
        for name in NSFW {
            assert!(is_nsfw(name));
            assert!(is_nsfw(&name.to_uppercase()));
        }
        assert!(!is_nsfw("definitely_not_a_real_nsfw_category_zzz"));
    }

    #[test]
    fn builtin_categories_each_non_empty() {
        for (name, items) in builtin_categories() {
            assert!(!items.is_empty(), "category '{}' has no items", name);
        }
    }

    #[test]
    fn builtin_category_names_is_sorted() {
        let names = builtin_category_names();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn builtin_category_names_matches_keys() {
        let cats = builtin_categories();
        let names = builtin_category_names();
        assert_eq!(names.len(), cats.len());
        for name in &names {
            assert!(cats.contains_key(name));
        }
    }

    #[test]
    fn known_categories_are_present() {
        let cats = builtin_categories();
        for expected in &["scientists", "elements", "planets", "colors"] {
            assert!(
                cats.contains_key(*expected),
                "missing category '{}'",
                expected
            );
        }
    }

    // ── categories.json integrity ─────────────────────────────────────────────

    #[test]
    fn embedded_json_parses() {
        // Panics with a clear message if categories.json is malformed.
        let _ = categories();
    }

    #[test]
    fn no_name_has_leading_or_trailing_whitespace() {
        for (cat, names) in builtin_categories() {
            for n in &names {
                assert_eq!(
                    n.trim(),
                    n,
                    "name '{}' in '{}' has surrounding whitespace",
                    n,
                    cat
                );
                assert!(!n.is_empty(), "empty name in category '{}'", cat);
            }
        }
    }

    #[test]
    fn no_duplicate_names_within_a_category() {
        for (cat, names) in builtin_categories() {
            let mut seen = std::collections::HashSet::new();
            for n in &names {
                assert!(
                    seen.insert(n.clone()),
                    "duplicate '{}' in category '{}'",
                    n,
                    cat
                );
            }
        }
    }

    #[test]
    fn every_name_fits_discord_nick_limit() {
        for (cat, names) in builtin_categories() {
            for n in &names {
                assert!(
                    n.chars().count() <= 32,
                    "name '{}' in '{}' exceeds Discord's 32-char nickname limit",
                    n,
                    cat
                );
            }
        }
    }

    #[test]
    fn cache_returns_stable_reference() {
        let a = categories() as *const _;
        let b = categories() as *const _;
        assert_eq!(
            a, b,
            "categories() should cache and return the same instance"
        );
    }
}
