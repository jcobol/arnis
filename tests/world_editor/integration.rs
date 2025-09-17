// Integration test that exercises the minimal WorldEditor workflow.
// It stubs required modules, places blocks across chunk boundaries and
// verifies the saved region NBT contains the expected block IDs and properties.
#[path = "../../src/block_definitions.rs"]
mod block_definitions;
#[path = "../../src/block_registry.rs"]
mod block_registry;
#[path = "../../src/colors.rs"]
mod colors;
#[path = "../../src/biome_definitions.rs"]
mod biome_definitions;
#[path = "../../src/biome_registry.rs"]
mod biome_registry;
#[path = "../../src/biomes.rs"]
mod biomes;

// Minimal stubs for modules referenced by world_editor.rs
mod coordinate_system {
    pub mod cartesian {
        #[derive(Clone, Copy)]
        pub struct XZBBox;
        impl XZBBox {
            pub fn contains(&self, _p: &XZPoint) -> bool {
                true
            }
            pub fn min_x(&self) -> i32 {
                0
            }
            pub fn min_z(&self) -> i32 {
                0
            }
            pub fn max_x(&self) -> i32 {
                31
            }
            pub fn max_z(&self) -> i32 {
                31
            }
        }
        #[derive(Clone, Copy)]
        pub struct XZPoint;
        impl XZPoint {
            pub fn new(_x: i32, _z: i32) -> Self {
                XZPoint
            }
        }
    }
    pub mod geographic {
        #[derive(Clone, Copy)]
        pub struct LLBBox;
        #[derive(Clone, Copy)]
        pub struct GeoPoint;
        impl GeoPoint {
            pub fn lat(&self) -> f64 {
                0.0
            }
            pub fn lng(&self) -> f64 {
                0.0
            }
        }
        impl LLBBox {
            pub fn min(&self) -> GeoPoint {
                GeoPoint
            }
            pub fn max(&self) -> GeoPoint {
                GeoPoint
            }
        }
    }
}

mod ground {
    #[derive(Clone)]
    pub struct Ground;
    impl Ground {
        pub fn ground_level(&self) -> i32 {
            0
        }
        pub fn level(&self, _p: crate::coordinate_system::cartesian::XZPoint) -> i32 {
            0
        }
    }
}

mod progress {
    pub fn emit_gui_progress_update(_progress: f64, _message: &str) {}
}

mod world_editor {
    use super::*;
    use fastnbt::Value;

    include!("../../src/world_editor.rs");

    /// Places blocks in different chunks and ensures they persist with the
    /// expected block IDs and property data after saving through WorldEditor.
    #[test]
    fn save_writes_blocks_with_properties() {
        use fastanvil::Region;
        use std::fs::File;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("region")).unwrap();

        let xzbbox = coordinate_system::cartesian::XZBBox;
        let llbbox = coordinate_system::geographic::LLBBox;
        let mut editor = WorldEditor::new(dir.path().to_path_buf(), &xzbbox, llbbox);

        // Block in chunk (0,0)
        editor.set_block_absolute(block_definitions::OAK_PLANKS, 1, 64, 1, None, None);

        // Block with properties in chunk (1,0)
        let mut sign_props = std::collections::HashMap::new();
        sign_props.insert("rotation".to_string(), Value::String("4".to_string()));
        sign_props.insert(
            "waterlogged".to_string(),
            Value::String("false".to_string()),
        );
        let sign_props_value = Value::Compound(sign_props.clone());
        editor.set_block_with_properties_absolute(
            block_definitions::BlockWithProperties::new(
                block_definitions::SIGN,
                Some(sign_props_value.clone()),
            ),
            17,
            64,
            1,
            None,
            None,
        );

        editor.save();

        let region_path = dir.path().join("region").join("r.0.0.mca");
        let mut region = Region::from_stream(File::open(region_path).unwrap()).unwrap();

        // Verify block in chunk (0,0)
        let chunk0_bytes = region.read_chunk(0, 0).unwrap().unwrap();
        let chunk0: Chunk = fastnbt::from_bytes(&chunk0_bytes).unwrap();
        let section0 = chunk0.sections.iter().find(|s| s.y == 4).unwrap();
        let data0 = section0
            .block_states
            .data
            .as_ref()
            .unwrap()
            .clone()
            .into_inner();
        let bits_per_block0 = data0.len() * 64 / 4096;
        let mask0 = (1u64 << bits_per_block0) - 1;
        let mut indices0 = Vec::with_capacity(4096);
        let mut iter0 = data0.iter();
        let mut cur0 = *iter0.next().unwrap() as u64;
        let mut cur_idx0 = 0;
        for _ in 0..4096 {
            if cur_idx0 + bits_per_block0 > 64 {
                cur0 = *iter0.next().unwrap() as u64;
                cur_idx0 = 0;
            }
            let p = ((cur0 >> cur_idx0) & mask0) as usize;
            cur_idx0 += bits_per_block0;
            indices0.push(p);
        }
        let idx0 = SectionToModify::index(1, 0, 1);
        let palette_idx0 = indices0[idx0];
        let item0 = &section0.block_states.palette[palette_idx0];
        assert_eq!(item0.name, "minecraft:oak_planks");
        assert!(item0.properties.is_none());

        // Verify block with properties in chunk (1,0)
        let chunk1_bytes = region.read_chunk(1, 0).unwrap().unwrap();
        let chunk1: Chunk = fastnbt::from_bytes(&chunk1_bytes).unwrap();
        let section1 = chunk1.sections.iter().find(|s| s.y == 4).unwrap();
        let data1 = section1
            .block_states
            .data
            .as_ref()
            .unwrap()
            .clone()
            .into_inner();
        let bits_per_block1 = data1.len() * 64 / 4096;
        let mask1 = (1u64 << bits_per_block1) - 1;
        let mut indices1 = Vec::with_capacity(4096);
        let mut iter1 = data1.iter();
        let mut cur1 = *iter1.next().unwrap() as u64;
        let mut cur_idx1 = 0;
        for _ in 0..4096 {
            if cur_idx1 + bits_per_block1 > 64 {
                cur1 = *iter1.next().unwrap() as u64;
                cur_idx1 = 0;
            }
            let p = ((cur1 >> cur_idx1) & mask1) as usize;
            cur_idx1 += bits_per_block1;
            indices1.push(p);
        }
        let idx1 = SectionToModify::index(1, 0, 1);
        let palette_idx1 = indices1[idx1];
        let item1 = &section1.block_states.palette[palette_idx1];
        assert_eq!(item1.name, "minecraft:oak_sign");
        assert_eq!(item1.properties, Some(sign_props_value));
    }

    #[test]
    fn save_writes_biome_palette_only() {
        use fastanvil::Region;
        use std::fs::File;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("region")).unwrap();

        let xzbbox = coordinate_system::cartesian::XZBBox;
        let llbbox = coordinate_system::geographic::LLBBox;
        let mut editor = WorldEditor::new(dir.path().to_path_buf(), &xzbbox, llbbox);

        editor.set_biome_absolute(biome_definitions::FOREST, 1, 64, 1);
        editor.save();

        let region_path = dir.path().join("region").join("r.0.0.mca");
        let mut region = Region::from_stream(File::open(region_path).unwrap()).unwrap();
        let chunk_bytes = region.read_chunk(0, 0).unwrap().unwrap();
        let chunk: Chunk = fastnbt::from_bytes(&chunk_bytes).unwrap();
        let section = chunk.sections.iter().find(|s| s.y == 4).unwrap();
        let biome_data = section.biomes.data.as_ref().unwrap().clone().into_inner();
        let entry_count = 64; // biomes stored per 4×4×4 cube
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
        let idx = SectionToModify::biome_index(1, 0, 1);
        let palette_idx = indices[idx];
        assert_eq!(section.biomes.palette[palette_idx], "minecraft:forest");
    }
}
