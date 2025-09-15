use std::collections::HashMap;

use crate::biome_definitions::{Biome, BEACH, FOREST, PLAINS, RIVER};

/// Determines a biome based on OSM-style tag key-value pairs.
///
/// Currently supports a handful of common tags with a fallback to
/// [`PLAINS`] when no specific biome mapping exists.
pub fn biome_from_tags(tags: &HashMap<String, String>) -> Option<Biome> {
    if let Some(value) = tags.get("landuse") {
        if value == "forest" {
            return Some(FOREST);
        }
    }

    if let Some(value) = tags.get("natural") {
        return match value.as_str() {
            "water" => Some(RIVER),
            "beach" => Some(BEACH),
            _ => Some(PLAINS),
        };
    }

    Some(PLAINS)
}
