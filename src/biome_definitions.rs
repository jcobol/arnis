use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
pub struct Biome {
    name: &'static str,
}

impl Biome {
    #[inline(always)]
    const fn new(namespaced_name: &'static str) -> Self {
        Self {
            name: namespaced_name,
        }
    }

    #[inline(always)]
    pub fn name(&self) -> &str {
        self.name
    }

    pub fn from_str(name: &str) -> Biome {
        let mut cache = BIOME_NAME_CACHE.lock().unwrap();
        if let Some(biome) = cache.get(name) {
            *biome
        } else {
            let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
            let biome = Biome::new(leaked);
            cache.insert(name.to_string(), biome);
            biome
        }
    }
}

static BIOME_NAME_CACHE: Lazy<Mutex<HashMap<String, Biome>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub const PLAINS: Biome = Biome::new("minecraft:plains");
pub const FOREST: Biome = Biome::new("minecraft:forest");
pub const RIVER: Biome = Biome::new("minecraft:river");
pub const BEACH: Biome = Biome::new("minecraft:beach");
pub const DESERT: Biome = Biome::new("minecraft:desert");
pub const OCEAN: Biome = Biome::new("minecraft:ocean");
pub const JUNGLE: Biome = Biome::new("minecraft:jungle");
pub const SWAMP: Biome = Biome::new("minecraft:swamp");
pub const TAIGA: Biome = Biome::new("minecraft:taiga");
pub const SAVANNA: Biome = Biome::new("minecraft:savanna");
pub const MOUNTAINS: Biome = Biome::new("minecraft:mountains");
