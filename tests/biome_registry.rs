#[path = "../src/biome_definitions.rs"]
#[allow(dead_code)]
mod biome_definitions;
#[path = "../src/biome_registry.rs"]
mod biome_registry;

use biome_definitions::*;
use biome_registry::*;

#[test]
fn known_biomes_have_stable_ids() {
    assert_eq!(id(PLAINS), 0);
    assert_eq!(id(FOREST), 1);
    assert_eq!(id(RIVER), 2);
}

#[test]
fn id_inserts_once_and_is_consistent() {
    let custom_name = "minecraft:__biome_registry_test";
    let first_id = id(Biome::from_str(custom_name));
    let second_id = id(Biome::from_str(custom_name));
    assert_eq!(first_id, second_id);

    let other_id = id(Biome::from_str("minecraft:__biome_registry_other_test"));
    assert_eq!(other_id, first_id + 1);
}

#[test]
fn biome_returns_original() {
    let custom = Biome::from_str("minecraft:__biome_registry_biome_test");
    let id = id(custom);
    assert_eq!(biome(id), custom);
}
