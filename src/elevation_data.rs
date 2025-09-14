use crate::coordinate_system::{geographic::LLBBox, transformation::geo_distance};
use image::Rgb;
use std::path::Path;

/// Maximum Y coordinate in Minecraft (build height limit)
const MAX_Y: i32 = 319;
/// Scale factor for converting real elevation to Minecraft heights
const BASE_HEIGHT_SCALE: f64 = 0.7;
/// AWS S3 Terrarium tiles endpoint (no API key required)
const AWS_TERRARIUM_URL: &str =
    "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png";
/// Terrarium format offset for height decoding
const TERRARIUM_OFFSET: f64 = 32768.0;
/// Minimum zoom level for terrain tiles
const MIN_ZOOM: u8 = 10;
/// Maximum zoom level for terrain tiles
const MAX_ZOOM: u8 = 15;

/// Holds processed elevation data and metadata
///
/// The elevation grid is stored in a flat `Vec<i16>` to reduce memory
/// consumption. Heights are stored in meters above sea level and converted to
/// Minecraft heights on demand.
#[derive(Clone)]
pub struct ElevationData {
    /// Raw elevation values in meters
    pub(crate) heights: Vec<i16>,
    /// Width of the elevation grid
    pub(crate) width: usize,
    /// Height of the elevation grid
    pub(crate) height: usize,
    /// Minimum raw elevation
    pub(crate) min_height: i16,
    /// Range of raw elevations (max - min)
    pub(crate) height_range: i16,
    /// Configured ground level
    pub(crate) ground_level: i32,
    /// Scaled range used for Minecraft conversion
    pub(crate) scaled_range: f64,
}

impl ElevationData {
    /// Returns the Minecraft Y level for the given grid coordinates
    #[inline]
    pub fn height_at(&self, x: usize, z: usize) -> i32 {
        let idx = z * self.width + x;
        let raw = self.heights[idx] as i32;
        let relative = (raw - self.min_height as i32) as f64 / self.height_range as f64;
        let scaled = relative * self.scaled_range;
        ((self.ground_level as f64 + scaled).round() as i32).clamp(self.ground_level, MAX_Y)
    }
}

/// Calculates appropriate zoom level for the given bounding box
fn calculate_zoom_level(bbox: &LLBBox) -> u8 {
    let lat_diff: f64 = (bbox.max().lat() - bbox.min().lat()).abs();
    let lng_diff: f64 = (bbox.max().lng() - bbox.min().lng()).abs();
    let max_diff: f64 = lat_diff.max(lng_diff);
    let zoom: u8 = (-max_diff.log2() + 20.0) as u8;
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

fn lat_lng_to_tile(lat: f64, lng: f64, zoom: u8) -> (u32, u32) {
    let lat_rad: f64 = lat.to_radians();
    let n: f64 = 2.0_f64.powi(zoom as i32);
    let x: u32 = ((lng + 180.0) / 360.0 * n).floor() as u32;
    let y: u32 = ((1.0 - lat_rad.tan().asinh() / std::f64::consts::PI) / 2.0 * n).floor() as u32;
    (x, y)
}

/// Downloads a tile from AWS Terrain Tiles service
fn download_tile(
    client: &reqwest::blocking::Client,
    tile_x: u32,
    tile_y: u32,
    zoom: u8,
    tile_path: &Path,
) -> Result<image::ImageBuffer<Rgb<u8>, Vec<u8>>, Box<dyn std::error::Error>> {
    println!("Fetching tile x={tile_x},y={tile_y},z={zoom} from AWS Terrain Tiles");
    let url: String = AWS_TERRARIUM_URL
        .replace("{z}", &zoom.to_string())
        .replace("{x}", &tile_x.to_string())
        .replace("{y}", &tile_y.to_string());

    let response: reqwest::blocking::Response = client.get(&url).send()?;
    response.error_for_status_ref()?;
    let bytes = response.bytes()?;
    std::fs::write(tile_path, &bytes)?;
    let img: image::DynamicImage = image::load_from_memory(&bytes)?;
    Ok(img.to_rgb8())
}

pub fn fetch_elevation_data(
    bbox: &LLBBox,
    scale: f64,
    ground_level: i32,
) -> Result<ElevationData, Box<dyn std::error::Error>> {
    let (base_scale_z, base_scale_x) = geo_distance(bbox.min(), bbox.max());

    // Apply same floor() and scale operations as CoordTransformer.llbbox_to_xzbbox()
    let scale_factor_z: f64 = base_scale_z.floor() * scale;
    let scale_factor_x: f64 = base_scale_x.floor() * scale;

    // Calculate zoom and tiles
    let zoom: u8 = calculate_zoom_level(bbox);
    let tiles: Vec<(u32, u32)> = get_tile_coordinates(bbox, zoom);

    // Match grid dimensions with Minecraft world size
    let grid_width: usize = scale_factor_x as usize;
    let grid_height: usize = scale_factor_z as usize;

    // Flat grid storing raw elevation values in meters
    let mut heights: Vec<i16> = vec![i16::MIN; grid_width * grid_height];

    let client: reqwest::blocking::Client = reqwest::blocking::Client::new();

    let tile_cache_dir = Path::new("./arnis-tile-cache");
    if !tile_cache_dir.exists() {
        std::fs::create_dir_all(tile_cache_dir)?;
    }

    // Fetch and process each tile row-by-row
    for (tile_x, tile_y) in &tiles {
        let tile_path = tile_cache_dir.join(format!("z{zoom}_x{tile_x}_y{tile_y}.png"));
        let rgb_img: image::ImageBuffer<Rgb<u8>, Vec<u8>> = if tile_path.exists() {
            match image::open(&tile_path) {
                Ok(img) => img.to_rgb8(),
                Err(_) => download_tile(&client, *tile_x, *tile_y, zoom, &tile_path)?,
            }
        } else {
            download_tile(&client, *tile_x, *tile_y, zoom, &tile_path)?
        };

        for (y, row) in rgb_img.rows().enumerate() {
            for (x, pixel) in row.enumerate() {
                // Convert tile pixel coordinates back to geographic coordinates
                let pixel_lng = ((*tile_x as f64 + x as f64 / 256.0) / (2.0_f64.powi(zoom as i32)))
                    * 360.0
                    - 180.0;
                let pixel_lat_rad = std::f64::consts::PI
                    * (1.0
                        - 2.0 * (*tile_y as f64 + y as f64 / 256.0) / (2.0_f64.powi(zoom as i32)));
                let pixel_lat = pixel_lat_rad.sinh().atan().to_degrees();

                if pixel_lat < bbox.min().lat()
                    || pixel_lat > bbox.max().lat()
                    || pixel_lng < bbox.min().lng()
                    || pixel_lng > bbox.max().lng()
                {
                    continue;
                }

                let rel_x = (pixel_lng - bbox.min().lng()) / (bbox.max().lng() - bbox.min().lng());
                let rel_y =
                    1.0 - (pixel_lat - bbox.min().lat()) / (bbox.max().lat() - bbox.min().lat());

                let scaled_x = (rel_x * grid_width as f64).round() as usize;
                let scaled_y = (rel_y * grid_height as f64).round() as usize;

                if scaled_y >= grid_height || scaled_x >= grid_width {
                    continue;
                }

                // Decode Terrarium format: (R * 256 + G + B/256) - 32768
                let height = (pixel[0] as f64 * 256.0 + pixel[1] as f64 + pixel[2] as f64 / 256.0)
                    - TERRARIUM_OFFSET;

                let idx = scaled_y * grid_width + scaled_x;
                heights[idx] = height.round() as i16;
            }
        }
    }

    // Replace any remaining empty cells with 0m and compute min/max
    let mut min_height: i16 = i16::MAX;
    let mut max_height: i16 = i16::MIN;
    for h in heights.iter_mut() {
        if *h == i16::MIN {
            *h = 0;
        }
        min_height = min_height.min(*h);
        max_height = max_height.max(*h);
    }

    let height_range: i16 = max_height - min_height;

    // Determine scaling similar to previous implementation
    let mut height_scale: f64 = BASE_HEIGHT_SCALE * scale.sqrt();
    let mut scaled_range: f64 = height_range as f64 * height_scale;

    let available_y_range = (MAX_Y - ground_level) as f64;
    let safety_margin = 0.9;
    let max_allowed_range = available_y_range * safety_margin;

    if scaled_range > max_allowed_range {
        let adjustment_factor = max_allowed_range / scaled_range;
        height_scale *= adjustment_factor;
        scaled_range = height_range as f64 * height_scale;
    }

    Ok(ElevationData {
        heights,
        width: grid_width,
        height: grid_height,
        min_height,
        height_range,
        ground_level,
        scaled_range,
    })
}

fn get_tile_coordinates(bbox: &LLBBox, zoom: u8) -> Vec<(u32, u32)> {
    // Convert lat/lng to tile coordinates
    let (x1, y1) = lat_lng_to_tile(bbox.min().lat(), bbox.min().lng(), zoom);
    let (x2, y2) = lat_lng_to_tile(bbox.max().lat(), bbox.max().lng(), zoom);

    let mut tiles: Vec<(u32, u32)> = Vec::new();
    for x in x1.min(x2)..=x1.max(x2) {
        for y in y1.min(y2)..=y1.max(y2) {
            tiles.push((x, y));
        }
    }
    tiles
}
