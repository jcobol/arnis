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
            pub fn min_x(&self) -> i32 { 0 }
            pub fn min_z(&self) -> i32 { 0 }
            pub fn max_x(&self) -> i32 { 0 }
            pub fn max_z(&self) -> i32 { 0 }
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
        #[derive(Clone, Copy)]
        pub struct GeoPoint;
        impl GeoPoint {
            pub fn lat(&self) -> f64 { 0.0 }
            pub fn lng(&self) -> f64 { 0.0 }
        }
        impl LLBBox {
            pub fn min(&self) -> GeoPoint { GeoPoint }
            pub fn max(&self) -> GeoPoint { GeoPoint }
        }
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

    include!("../../src/world_editor.rs");

    #[test]
    fn to_section_serialization_round_trip() {
        let mut section = SectionToModify::default();

        // 15 unique blocks without extra properties
        let blocks = [
            block_definitions::ACACIA_PLANKS,
            block_definitions::ANDESITE,
            block_definitions::BIRCH_LEAVES,
            block_definitions::BIRCH_LOG,
            block_definitions::BLACK_CONCRETE,
            block_definitions::BLACKSTONE,
            block_definitions::BLUE_FLOWER,
            block_definitions::BLUE_TERRACOTTA,
            block_definitions::BRICK,
            block_definitions::CAULDRON,
            block_definitions::CHISELED_STONE_BRICKS,
            block_definitions::COBBLESTONE_WALL,
            block_definitions::COBBLESTONE,
            block_definitions::POLISHED_BLACKSTONE_BRICKS,
            block_definitions::CRACKED_STONE_BRICKS,
        ];

        let mut expected: Vec<(usize, String, Option<fastnbt::Value>)> = Vec::new();

        for (i, &block) in blocks.iter().enumerate() {
            section.set_block(i as u8, 0, 0, block);
            let idx = SectionToModify::index(i as u8, 0, 0);
            expected.push((idx, block.name().to_string(), None));
        }

        // Block with custom properties: sign rotation 4
        let mut sign_props = std::collections::HashMap::new();
        sign_props.insert(
            "rotation".to_string(),
            fastnbt::Value::String("4".to_string()),
        );
        let sign_props_value = fastnbt::Value::Compound(sign_props.clone());
        section.set_block_with_properties(
            0,
            1,
            0,
            block_definitions::BlockWithProperties::new(
                block_definitions::SIGN,
                Some(sign_props_value.clone()),
            ),
        );
        expected.push((
            SectionToModify::index(0, 1, 0),
            "minecraft:oak_sign".to_string(),
            Some(sign_props_value),
        ));

        // Block with custom properties: trapdoor half bottom
        let mut trap_props = std::collections::HashMap::new();
        trap_props.insert(
            "half".to_string(),
            fastnbt::Value::String("bottom".to_string()),
        );
        let trap_props_value = fastnbt::Value::Compound(trap_props.clone());
        section.set_block_with_properties(
            1,
            1,
            0,
            block_definitions::BlockWithProperties::new(
                block_definitions::OAK_TRAPDOOR,
                Some(trap_props_value.clone()),
            ),
        );
        expected.push((
            SectionToModify::index(1, 1, 0),
            "minecraft:oak_trapdoor".to_string(),
            Some(trap_props_value),
        ));

        let nbt_section = section.to_section(0);

        let palette_len = nbt_section.block_states.palette.len();
        assert!(palette_len > 16);
        assert_eq!(palette_len, expected.len() + 1); // + air

        let data = nbt_section
            .block_states
            .data
            .as_ref()
            .expect("block state data")
            .clone()
            .into_inner();
        let bits_per_block = data.len() * 64 / 4096;
        assert_eq!(bits_per_block, 5);

        // Decode indices from bitpacked data
        let mask = (1u64 << bits_per_block) - 1;
        let mut indices = Vec::with_capacity(4096);
        let mut iter = data.iter();
        let mut cur = *iter.next().unwrap() as u64;
        let mut cur_idx = 0;
        for _ in 0..4096 {
            if cur_idx + bits_per_block > 64 {
                cur = *iter.next().unwrap() as u64;
                cur_idx = 0;
            }
            let p = ((cur >> cur_idx) & mask) as usize;
            cur_idx += bits_per_block;
            indices.push(p);
        }

        for (idx, name, props) in expected {
            let palette_idx = indices[idx];
            let item = &nbt_section.block_states.palette[palette_idx];
            assert_eq!(item.name, name);
            if let Some(p) = props {
                assert_eq!(item.properties, Some(p));
            }
        }

        // Verify an untouched block is air
        let air_idx = SectionToModify::index(15, 15, 15);
        let air_palette_idx = indices[air_idx];
        let air_item = &nbt_section.block_states.palette[air_palette_idx];
        assert_eq!(air_item.name, "minecraft:air");

        assert_eq!(nbt_section.biomes.palette, vec!["minecraft:plains".to_string()]);
        assert!(nbt_section.biomes.data.is_none());
    }

    #[test]
    fn biome_serialization_writes_palette_and_data() {
        let mut section = SectionToModify::default();
        section.set_biome(0, 0, 0, biome_definitions::DESERT);
        let nbt_section = section.to_section(0);
        assert_eq!(nbt_section.biomes.palette.len(), 2);
        assert!(nbt_section.biomes.palette.contains(&"minecraft:plains".to_string()));
        assert!(nbt_section.biomes.palette.contains(&"minecraft:desert".to_string()));
        let biome_data = nbt_section
            .biomes
            .data
            .as_ref()
            .expect("biome data")
            .clone()
            .into_inner();
        let entry_count = 64; // biomes stored on a 4×4×4 grid
        let bits_per_biome = biome_data.len() * 64 / entry_count;
        let mask = (1u64 << bits_per_biome) - 1;
        let mut indices = Vec::with_capacity(entry_count);
        let mut iter = biome_data.iter();
        let mut cur = *iter.next().unwrap() as u64;
        let mut cur_idx = 0;
        for _ in 0..entry_count {
            if cur_idx + bits_per_biome > 64 {
                cur = *iter.next().unwrap() as u64;
                cur_idx = 0;
            }
            let p = ((cur >> cur_idx) & mask) as usize;
            cur_idx += bits_per_biome;
            indices.push(p);
        }
        let idx = SectionToModify::biome_index(0, 0, 0);
        let palette_idx = indices[idx];
        assert_eq!(nbt_section.biomes.palette[palette_idx], "minecraft:desert");
    }
}
