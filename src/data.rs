use std::collections::HashMap;

/// Return a fresh copy of the built-in name categories.
pub fn builtin_categories() -> HashMap<String, Vec<String>> {
    let raw: &[(&str, &[&str])] = &[
        (
            "scientists",
            &[
                "Einstein", "Newton", "Darwin", "Curie", "Tesla", "Feynman", "Hawking", "Bohr",
                "Faraday", "Turing", "Heisenberg", "Schrodinger", "Planck", "Dirac", "Fermi",
                "Oppenheimer", "Lovelace", "Noether", "Ramanujan", "Euler",
            ],
        ),
        (
            "elements",
            &[
                "Hydrogen", "Helium", "Lithium", "Carbon", "Nitrogen", "Oxygen", "Neon",
                "Sodium", "Magnesium", "Aluminum", "Silicon", "Phosphorus", "Sulfur", "Chlorine",
                "Argon", "Potassium", "Calcium", "Iron", "Copper", "Zinc", "Silver", "Gold",
                "Mercury", "Lead", "Uranium",
            ],
        ),
        (
            "chemical_compounds",
            &[
                "Caffeine", "Aspirin", "Serotonin", "Dopamine", "Adrenaline", "Glucose",
                "Ethanol", "Acetone", "Benzene", "Toluene", "Acetylene", "Propane", "Methane",
                "Ozone", "Ammonia", "Sucrose", "Fructose", "Lactose", "Galactose", "Maltose",
            ],
        ),
        (
            "amusement_parks",
            &[
                "Disneyland",
                "Six Flags",
                "Cedar Point",
                "Universal Studios",
                "Busch Gardens",
                "Knott's Berry Farm",
                "Hersheypark",
                "SeaWorld",
                "Dollywood",
                "Alton Towers",
                "Europa Park",
                "PortAventura",
                "Thorpe Park",
                "Legoland",
                "Dreamworld",
                "Tivoli Gardens",
                "Phantasialand",
                "Holiday World",
                "Carowinds",
                "Adventureland",
            ],
        ),
        (
            "dinosaurs",
            &[
                "T-Rex",
                "Velociraptor",
                "Triceratops",
                "Stegosaurus",
                "Brachiosaurus",
                "Ankylosaurus",
                "Pterodactyl",
                "Diplodocus",
                "Allosaurus",
                "Spinosaurus",
                "Iguanodon",
                "Pachycephalosaurus",
                "Parasaurolophus",
                "Gallimimus",
                "Carnotaurus",
                "Dilophosaurus",
                "Ceratosaurus",
                "Compsognathus",
                "Maiasaura",
                "Oviraptor",
            ],
        ),
        (
            "planets",
            &[
                "Mercury", "Venus", "Earth", "Mars", "Jupiter", "Saturn", "Uranus", "Neptune",
                "Pluto", "Ceres", "Eris", "Makemake", "Haumea", "Sedna", "Quaoar",
            ],
        ),
        (
            "colors",
            &[
                "Crimson", "Scarlet", "Vermilion", "Tangerine", "Amber", "Chartreuse",
                "Viridian", "Cerulean", "Cobalt", "Indigo", "Violet", "Magenta", "Fuchsia",
                "Turquoise", "Periwinkle", "Sienna", "Ochre", "Umber", "Sepia", "Ecru",
            ],
        ),
        (
            "fruits",
            &[
                "Mango",
                "Papaya",
                "Persimmon",
                "Pomegranate",
                "Lychee",
                "Dragonfruit",
                "Starfruit",
                "Jackfruit",
                "Durian",
                "Guava",
                "Passion Fruit",
                "Tamarind",
                "Kumquat",
                "Rambutan",
                "Soursop",
                "Ackee",
                "Feijoa",
                "Cherimoya",
                "Langsat",
                "Salak",
            ],
        ),
    ];

    raw.iter()
        .map(|(cat, names)| {
            (
                cat.to_string(),
                names.iter().map(|n| n.to_string()).collect(),
            )
        })
        .collect()
}

/// Built-in category names as a sorted list (for display).
pub fn builtin_category_names() -> Vec<String> {
    let mut names: Vec<String> = builtin_categories().into_keys().collect();
    names.sort();
    names
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
            assert!(cats.contains_key(*expected), "missing category '{}'", expected);
        }
    }
}
