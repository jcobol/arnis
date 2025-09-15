#[path = "../../src/colors.rs"]
mod colors;
#[path = "../../src/block_definitions.rs"]
mod block_definitions;
#[path = "../../src/block_registry.rs"]
mod block_registry;
#[path = "../../src/biome_definitions.rs"]
mod biome_definitions;
#[path = "../../src/biome_registry.rs"]
mod biome_registry;

// Minimal stubs for modules referenced by world_editor.rs
mod coordinate_system {
    pub mod cartesian {
        #[derive(Clone, Copy)]
        pub struct XZBBox;
        impl XZBBox {
            pub fn contains(&self, _p: &XZPoint) -> bool { true }
        }
        #[derive(Clone, Copy)]
        pub struct XZPoint;
        impl XZPoint {
            pub fn new(_x: i32, _z: i32) -> Self { XZPoint }
        }
    }
    pub mod geographic {
        #[derive(Clone, Copy)]
        pub struct LLBBox;
    }
}

mod ground {
    #[derive(Clone)]
    pub struct Ground;
    impl Ground {
        pub fn ground_level(&self) -> i32 { 0 }
        pub fn level(&self, _p: crate::coordinate_system::cartesian::XZPoint) -> i32 { 0 }
    }
}

mod progress {
    pub fn emit_gui_progress_update(_a: u64, _b: u64) {}
}

mod world_editor {
    use super::*;
    use fastnbt::Value;
    use std::collections::HashMap;

    include!("../../src/world_editor.rs");

    #[test]
    fn set_block_get_block_round_trip() {
        let mut section = SectionToModify::default();
        section.set_block(1, 2, 3, block_definitions::OAK_PLANKS);
        assert_eq!(section.get_block(1, 2, 3), Some(block_definitions::OAK_PLANKS));
    }

    #[test]
    fn set_block_with_properties_maintains_properties() {
        let mut section = SectionToModify::default();
        let mut props = match block_definitions::SIGN.properties() {
            Some(Value::Compound(map)) => map,
            _ => HashMap::new(),
        };
        props.insert("rotation".to_string(), Value::String("4".to_string()));
        let sign_block = block_definitions::BlockWithProperties::new(
            block_definitions::SIGN,
            Some(Value::Compound(props.clone())),
        );
        section.set_block_with_properties(0, 0, 0, sign_block);

        let nbt_section = section.to_section(0);
        let sign_palette = nbt_section
            .block_states
            .palette
            .iter()
            .find(|p| p.name == "minecraft:oak_sign")
            .expect("sign palette entry");
        match &sign_palette.properties {
            Some(Value::Compound(map)) => {
                assert_eq!(map.get("rotation"), Some(&Value::String("4".to_string())));
            }
            _ => panic!("sign properties missing"),
        }
    }

    #[test]
    fn default_initialization_fills_with_air() {
        let section = SectionToModify::default();
        assert!(section.block_ids.iter().all(|&id| id == block_registry::AIR_ID));
    }

    #[test]
    fn set_biome_stores_id() {
        let mut section = SectionToModify::default();
        section.set_biome(1, 2, 3, biome_definitions::FOREST);
        let idx = SectionToModify::index(1, 2, 3);
        assert_eq!(
            section.biome_ids[idx],
            biome_registry::id(biome_definitions::FOREST)
        );
    }

    #[test]
    fn default_biomes_are_plains() {
        let section = SectionToModify::default();
        let plains_id = biome_registry::id(biome_definitions::PLAINS);
        assert!(section.biome_ids.iter().all(|&id| id == plains_id));
    }
}

