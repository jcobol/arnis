use crate::args::Args;
use crate::coordinate_system::{cartesian::XZPoint, geographic::LLBBox};
use crate::elevation_data::{fetch_elevation_data, ElevationData};
use crate::progress::emit_gui_progress_update;
use colored::Colorize;
use image::{Rgb, RgbImage};

/// Represents terrain data and elevation settings
#[derive(Clone)]
pub struct Ground {
    pub elevation_enabled: bool,
    ground_level: i32,
    elevation_data: Option<ElevationData>,
}

impl Ground {
    pub fn new_flat(ground_level: i32) -> Self {
        Self {
            elevation_enabled: false,
            ground_level,
            elevation_data: None,
        }
    }

    pub fn new_enabled(bbox: &LLBBox, scale: f64, ground_level: i32) -> Self {
        let elevation_data = fetch_elevation_data(bbox, scale, ground_level)
            .expect("Failed to fetch elevation data");
        Self {
            elevation_enabled: true,
            ground_level,
            elevation_data: Some(elevation_data),
        }
    }

    /// Returns the ground level at the given coordinates
    #[inline(always)]
    pub fn level(&self, coord: XZPoint) -> i32 {
        if !self.elevation_enabled || self.elevation_data.is_none() {
            return self.ground_level;
        }

        let data: &ElevationData = self.elevation_data.as_ref().unwrap();
        let (x_ratio, z_ratio) = self.get_data_coordinates(coord, data);
        self.interpolate_height(x_ratio, z_ratio, data)
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn min_level<I: Iterator<Item = XZPoint>>(&self, coords: I) -> Option<i32> {
        if !self.elevation_enabled {
            return Some(self.ground_level);
        }
        coords.map(|c: XZPoint| self.level(c)).min()
    }

    #[allow(unused)]
    #[inline(always)]
    pub fn max_level<I: Iterator<Item = XZPoint>>(&self, coords: I) -> Option<i32> {
        if !self.elevation_enabled {
            return Some(self.ground_level);
        }
        coords.map(|c: XZPoint| self.level(c)).max()
    }

    /// Returns the configured ground level regardless of elevation data
    #[inline(always)]
    pub fn ground_level(&self) -> i32 {
        self.ground_level
    }

    /// Converts game coordinates to elevation data coordinates
    #[inline(always)]
    fn get_data_coordinates(&self, coord: XZPoint, data: &ElevationData) -> (f64, f64) {
        let x_ratio: f64 = coord.x as f64 / data.width as f64;
        let z_ratio: f64 = coord.z as f64 / data.height as f64;
        (x_ratio.clamp(0.0, 1.0), z_ratio.clamp(0.0, 1.0))
    }

    /// Interpolates height value from the elevation grid
    #[inline(always)]
    fn interpolate_height(&self, x_ratio: f64, z_ratio: f64, data: &ElevationData) -> i32 {
        let x: usize = ((x_ratio * (data.width - 1) as f64).round() as usize).min(data.width - 1);
        let z: usize = ((z_ratio * (data.height - 1) as f64).round() as usize).min(data.height - 1);
        data.height_at(x, z)
    }

    fn save_debug_image(&self, filename: &str) {
        let data = self
            .elevation_data
            .as_ref()
            .expect("Elevation data not available");
        if data.heights.is_empty() {
            return;
        }

        let height: usize = data.height;
        let width: usize = data.width;
        let mut img: image::ImageBuffer<Rgb<u8>, Vec<u8>> =
            RgbImage::new(width as u32, height as u32);

        let mut min_height: i32 = i32::MAX;
        let mut max_height: i32 = i32::MIN;

        for z in 0..height {
            for x in 0..width {
                let h = data.height_at(x, z);
                min_height = min_height.min(h);
                max_height = max_height.max(h);
            }
        }

        for z in 0..height {
            for x in 0..width {
                let h = data.height_at(x, z);
                let normalized: u8 =
                    (((h - min_height) as f64 / (max_height - min_height) as f64) * 255.0) as u8;
                img.put_pixel(
                    x as u32,
                    z as u32,
                    Rgb([normalized, normalized, normalized]),
                );
            }
        }

        // Ensure filename has .png extension
        let filename: String = if !filename.ends_with(".png") {
            format!("{filename}.png")
        } else {
            filename.to_string()
        };

        if let Err(e) = img.save(&filename) {
            eprintln!("Failed to save debug image: {e}");
        }
    }
}

#[cfg(test)]
impl Ground {
    pub fn from_heights(ground_level: i32, heights: Vec<Vec<i32>>) -> Self {
        let height = heights.len();
        let width = heights.first().map(|r| r.len()).unwrap_or(0);
        let flat: Vec<i16> = heights
            .into_iter()
            .flat_map(|r| r.into_iter().map(|v| v as i16))
            .collect();
        // Use identity scaling so that height_at returns the raw values
        let min_height = 0;
        let height_range = 1;
        Self {
            elevation_enabled: true,
            ground_level,
            elevation_data: Some(ElevationData {
                heights: flat,
                width,
                height,
                min_height,
                height_range,
                ground_level,
                scaled_range: 1.0,
            }),
        }
    }
}

pub fn generate_ground_data(args: &Args) -> Ground {
    if args.terrain {
        println!("{} Fetching elevation...", "[3/7]".bold());
        emit_gui_progress_update(15.0, "Fetching elevation...");
        let ground = Ground::new_enabled(&args.bbox, args.scale, args.ground_level);
        if args.debug {
            ground.save_debug_image("elevation_debug");
        }
        return ground;
    }
    Ground::new_flat(args.ground_level)
}
