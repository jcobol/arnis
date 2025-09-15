//! Maintains a bidirectional mapping between [`Biome`] values and compact
//! `u16` identifiers used for referencing biomes.

use fnv::FnvHashMap;
use once_cell::sync::Lazy;
use std::sync::Mutex;

use crate::biome_definitions::Biome;
use crate::biome_definitions::*;

struct Registry {
    biomes: Vec<Biome>,
    ids: FnvHashMap<Biome, u16>,
}

static REGISTRY: Lazy<Mutex<Registry>> = Lazy::new(|| {
    let biomes = vec![
        PLAINS, FOREST, RIVER, BEACH, DESERT, OCEAN, JUNGLE, SWAMP, TAIGA, SAVANNA, MOUNTAINS,
    ];
    let mut ids = FnvHashMap::default();
    for (id, biome) in biomes.iter().copied().enumerate() {
        ids.insert(biome, id as u16);
    }
    Mutex::new(Registry { biomes, ids })
});

pub fn id(biome: Biome) -> u16 {
    let mut registry = REGISTRY.lock().unwrap();
    if let Some(&id) = registry.ids.get(&biome) {
        id
    } else {
        let id = registry.biomes.len() as u16;
        registry.biomes.push(biome);
        registry.ids.insert(biome, id);
        id
    }
}

pub fn biome(id: u16) -> Biome {
    let registry = REGISTRY.lock().unwrap();
    registry
        .biomes
        .get(id as usize)
        .copied()
        .expect("biome id out of range")
}
