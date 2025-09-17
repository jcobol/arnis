use std::collections::HashMap;

use crate::biome_definitions::{
    Biome, BEACH, DESERT, FOREST, JUNGLE, MOUNTAINS, MUSHROOM_FIELDS, OCEAN, PLAINS, RIVER,
    SAVANNA, SNOWY_TAIGA, SNOWY_TUNDRA, SWAMP, TAIGA,
};

struct Mapping {
    value: &'static str,
    biome: Biome,
}

const LANDUSE_MAPPINGS: &[Mapping] = &[
    Mapping {
        value: "forest",
        biome: FOREST,
    },
    Mapping {
        value: "grass",
        biome: PLAINS,
    },
    Mapping {
        value: "meadow",
        biome: PLAINS,
    },
    Mapping {
        value: "greenfield",
        biome: PLAINS,
    },
    Mapping {
        value: "orchard",
        biome: FOREST,
    },
    Mapping {
        value: "farmland",
        biome: PLAINS,
    },
    Mapping {
        value: "military",
        biome: PLAINS,
    },
    Mapping {
        value: "industrial",
        biome: PLAINS,
    },
    Mapping {
        value: "railway",
        biome: PLAINS,
    },
    Mapping {
        value: "commercial",
        biome: PLAINS,
    },
    Mapping {
        value: "residential",
        biome: PLAINS,
    },
    Mapping {
        value: "cemetery",
        biome: PLAINS,
    },
    Mapping {
        value: "traffic_island",
        biome: PLAINS,
    },
    Mapping {
        value: "construction",
        biome: PLAINS,
    },
    Mapping {
        value: "village_green",
        biome: PLAINS,
    },
];

const NATURAL_MAPPINGS: &[Mapping] = &[
    Mapping {
        value: "beach",
        biome: BEACH,
    },
    Mapping {
        value: "coastline",
        biome: BEACH,
    },
    Mapping {
        value: "wetland",
        biome: SWAMP,
    },
    Mapping {
        value: "swamp",
        biome: SWAMP,
    },
    Mapping {
        value: "marsh",
        biome: SWAMP,
    },
    Mapping {
        value: "wood",
        biome: FOREST,
    },
    Mapping {
        value: "scrub",
        biome: SAVANNA,
    },
    Mapping {
        value: "grassland",
        biome: SAVANNA,
    },
    Mapping {
        value: "heath",
        biome: SAVANNA,
    },
    Mapping {
        value: "taiga",
        biome: TAIGA,
    },
    Mapping {
        value: "fell",
        biome: MOUNTAINS,
    },
    Mapping {
        value: "bare_rock",
        biome: MOUNTAINS,
    },
    Mapping {
        value: "scree",
        biome: MOUNTAINS,
    },
    Mapping {
        value: "rock",
        biome: MOUNTAINS,
    },
    Mapping {
        value: "sand",
        biome: DESERT,
    },
    Mapping {
        value: "glacier",
        biome: SNOWY_TUNDRA,
    },
    Mapping {
        value: "ice",
        biome: SNOWY_TUNDRA,
    },
    Mapping {
        value: "tree",
        biome: FOREST,
    },
    Mapping {
        value: "woodland",
        biome: FOREST,
    },
];

const LEISURE_MAPPINGS: &[Mapping] = &[
    Mapping {
        value: "park",
        biome: PLAINS,
    },
    Mapping {
        value: "nature_reserve",
        biome: FOREST,
    },
    Mapping {
        value: "pitch",
        biome: PLAINS,
    },
    Mapping {
        value: "golf_course",
        biome: PLAINS,
    },
    Mapping {
        value: "garden",
        biome: PLAINS,
    },
];

const KNOWN_BIOMES: &[(&str, Biome)] = &[
    ("minecraft:plains", PLAINS),
    ("minecraft:forest", FOREST),
    ("minecraft:river", RIVER),
    ("minecraft:beach", BEACH),
    ("minecraft:desert", DESERT),
    ("minecraft:ocean", OCEAN),
    ("minecraft:jungle", JUNGLE),
    ("minecraft:swamp", SWAMP),
    ("minecraft:taiga", TAIGA),
    ("minecraft:savanna", SAVANNA),
    ("minecraft:mountains", MOUNTAINS),
    ("minecraft:snowy_tundra", SNOWY_TUNDRA),
    ("minecraft:snowy_taiga", SNOWY_TAIGA),
    ("minecraft:mushroom_fields", MUSHROOM_FIELDS),
];

fn parse_known_biome(name: &str) -> Option<Biome> {
    KNOWN_BIOMES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, biome)| *biome)
}

fn lookup(table: &[Mapping], value: &str) -> Option<Biome> {
    table
        .iter()
        .find(|mapping| mapping.value == value)
        .map(|mapping| mapping.biome)
}

fn biome_from_water_related(tags: &HashMap<String, String>) -> Option<Biome> {
    if let Some(water_value) = tags.get("water") {
        return match water_value.as_str() {
            "river" | "canal" | "stream" => Some(RIVER),
            "lake" | "reservoir" | "lagoon" | "pond" => Some(OCEAN),
            "sea" | "ocean" => Some(OCEAN),
            "wetland" | "swamp" => Some(SWAMP),
            _ => Some(RIVER),
        };
    }

    if let Some(waterway) = tags.get("waterway") {
        return match waterway.as_str() {
            "river" | "canal" | "stream" => Some(RIVER),
            "drain" => Some(SWAMP),
            _ => Some(RIVER),
        };
    }

    None
}

/// Determines a biome based on OSM-style tag key-value pairs.
///
/// The priority order is explicit biome tag, natural feature, water-specific
/// hints, then landuse/leisure fallbacks. If nothing matches we return
/// [`PLAINS`].
pub fn biome_from_tags(tags: &HashMap<String, String>) -> Option<Biome> {
    if let Some(custom) = tags.get("biome") {
        if let Some(biome) = parse_known_biome(custom) {
            return Some(biome);
        }
    }

    if let Some(natural_value) = tags.get("natural") {
        if natural_value == "water" {
            if let Some(water_biome) = biome_from_water_related(tags) {
                return Some(water_biome);
            }
        }

        if let Some(biome) = lookup(NATURAL_MAPPINGS, natural_value) {
            return Some(biome);
        }
    }

    // If no natural tags gave us a biome, consider water-specific hints that
    // may come from `landuse=reservoir` style tagging where natural is absent.
    if let Some(water_biome) = biome_from_water_related(tags) {
        return Some(water_biome);
    }

    if let Some(landuse_value) = tags.get("landuse") {
        if let Some(biome) = lookup(LANDUSE_MAPPINGS, landuse_value) {
            return Some(biome);
        }
    }

    if let Some(leisure_value) = tags.get("leisure") {
        if let Some(biome) = lookup(LEISURE_MAPPINGS, leisure_value) {
            return Some(biome);
        }
    }

    Some(PLAINS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome_definitions::{BEACH, OCEAN};

    #[test]
    fn forest_from_landuse() {
        let mut tags = HashMap::new();
        tags.insert("landuse".to_string(), "forest".to_string());
        assert_eq!(biome_from_tags(&tags), Some(FOREST));
    }

    #[test]
    fn river_from_water_tag() {
        let mut tags = HashMap::new();
        tags.insert("natural".to_string(), "water".to_string());
        tags.insert("water".to_string(), "river".to_string());
        assert_eq!(biome_from_tags(&tags), Some(RIVER));
    }

    #[test]
    fn ocean_from_lake() {
        let mut tags = HashMap::new();
        tags.insert("water".to_string(), "lake".to_string());
        assert_eq!(biome_from_tags(&tags), Some(OCEAN));
    }

    #[test]
    fn beach_from_natural() {
        let mut tags = HashMap::new();
        tags.insert("natural".to_string(), "beach".to_string());
        assert_eq!(biome_from_tags(&tags), Some(BEACH));
    }

    #[test]
    fn leisure_park_falls_back() {
        let mut tags = HashMap::new();
        tags.insert("leisure".to_string(), "park".to_string());
        assert_eq!(biome_from_tags(&tags), Some(PLAINS));
    }

    #[test]
    fn default_is_plains() {
        let tags = HashMap::<String, String>::new();
        assert_eq!(biome_from_tags(&tags), Some(PLAINS));
    }

    #[test]
    fn explicit_biome_tag_takes_priority() {
        let mut tags = HashMap::new();
        tags.insert("biome".to_string(), "minecraft:mushroom_fields".to_string());
        tags.insert("landuse".to_string(), "forest".to_string());
        assert_eq!(biome_from_tags(&tags), Some(MUSHROOM_FIELDS));
    }

    #[test]
    fn waterway_without_natural_is_river() {
        let mut tags = HashMap::new();
        tags.insert("waterway".to_string(), "river".to_string());
        assert_eq!(biome_from_tags(&tags), Some(RIVER));
    }

    #[test]
    fn wetland_prefers_swamp() {
        let mut tags = HashMap::new();
        tags.insert("natural".to_string(), "water".to_string());
        tags.insert("water".to_string(), "wetland".to_string());
        assert_eq!(biome_from_tags(&tags), Some(SWAMP));
    }

    #[test]
    fn scrub_leads_to_savanna() {
        let mut tags = HashMap::new();
        tags.insert("natural".to_string(), "scrub".to_string());
        assert_eq!(biome_from_tags(&tags), Some(SAVANNA));
    }

    // Ensure mountains mapping picks the correct biome.
    #[test]
    fn mountain_features_map_to_mountains() {
        let feature_values = ["fell", "bare_rock", "scree", "rock"];
        for value in feature_values {
            let mut tags = HashMap::new();
            tags.insert("natural".to_string(), value.to_string());
            assert_eq!(biome_from_tags(&tags), Some(MOUNTAINS));
        }
    }
}
