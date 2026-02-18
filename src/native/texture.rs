use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use vulkano::image::Image;

/// Information about a single sprite within an atlas
#[derive(Debug, Clone)]
pub struct SpriteInfo {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

/// Atlas information containing texture handle and its sprites
#[derive(Debug, Clone)]
pub struct AtlasInfo {
    pub texture: Arc<Image>,
    pub atlas_name: String,
    pub sprites: HashMap<String, SpriteInfo>,
}

/// Manager for atlas textures and their sprite mappings
#[derive(Clone)]
pub struct AtlasManager {
    pub atlases: Arc<RwLock<HashMap<String, AtlasInfo>>>,
}

impl AtlasManager {
    pub fn new() -> Self {
        Self {
            atlases: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register or update an atlas with its sprite information
    pub fn sync_atlas(
        &self,
        texture: Arc<Image>,
        atlas_name: String,
        sprites: HashMap<String, SpriteInfo>,
    ) {
        let atlas_info = AtlasInfo {
            texture,
            atlas_name: atlas_name.clone(),
            sprites,
        };
        self.atlases.write().unwrap().insert(atlas_name, atlas_info);
    }

    /// Get atlas information by texture handle
    pub fn get_atlas(&self, atlas_name: String) -> Option<AtlasInfo> {
        self.atlases.read().unwrap().get(&atlas_name).cloned()
    }

    /// Remove an atlas from management
    pub fn remove_atlas(&self, atlas_name: String) {
        self.atlases.write().unwrap().remove(&atlas_name);
    }
}

impl Default for AtlasManager {
    fn default() -> Self {
        Self::new()
    }
}
