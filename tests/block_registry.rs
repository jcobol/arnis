#[path = "../src/block_definitions.rs"]
mod block_definitions;
#[path = "../src/block_registry.rs"]
mod block_registry;
#[path = "../src/colors.rs"]
mod colors;

use block_definitions::*;
use block_registry::*;

#[test]
fn known_blocks_have_stable_ids() {
    assert_eq!(id(AIR), AIR_ID);
    assert_eq!(id(STONE), 84);
    assert_eq!(id(WATER), 87);
}

#[test]
fn id_inserts_once_and_is_consistent() {
    let custom_name = "minecraft:__block_registry_test";
    let first_id = id(Block::from_str(custom_name));
    let second_id = id(Block::from_str(custom_name));
    assert_eq!(first_id, second_id);

    let other_id = id(Block::from_str("minecraft:__block_registry_other_test"));
    assert_eq!(other_id, first_id + 1);
}

#[test]
fn block_returns_original() {
    let custom = Block::from_str("minecraft:__block_registry_block_test");
    let id = id(custom);
    assert_eq!(block(id), custom);
}
