use crate::block_definitions::*;
use crate::bresenham::bresenham_line;
use crate::osm_parser::ProcessedWay;
use crate::world_editor::WorldEditor;
use std::collections::HashMap;

/// Parse a width string which may contain units and return the width in
/// Minecraft blocks (approximately meters).
fn parse_width_to_blocks(width_str: &str) -> Option<i32> {
    let mut number_part = String::new();
    let mut unit_part = String::new();

    for c in width_str.trim().chars() {
        if c.is_ascii_digit() || c == '.' || c == ',' {
            if c == ',' {
                number_part.push('.');
            } else {
                number_part.push(c);
            }
        } else if !c.is_whitespace() {
            unit_part.push(c.to_ascii_lowercase());
        }
    }

    let value: f32 = number_part.parse().ok()?;
    let meters = if unit_part.contains("ft")
        || unit_part.contains("foot")
        || unit_part.contains("feet")
        || unit_part.contains("'")
    {
        value * 0.3048
    } else if unit_part.contains("km") {
        value * 1000.0
    } else {
        // Default assume meters
        value
    };

    Some(meters.round().max(1.0) as i32)
}

fn infer_width_from_tags(tags: &HashMap<String, String>, default: i32) -> i32 {
    if let Some(width_str) = tags.get("width") {
        if let Some(width) = parse_width_to_blocks(width_str) {
            return width;
        }
    }

    // Alternative metadata keys that may specify the width, including data
    // copied from an associated riverbank polygon.
    let alternative_keys = [
        "riverbank:width",
        "riverbank_width",
        "est_width",
        "estimated_width",
        "avg_width",
        "average_width",
        "width:avg",
        "width:est",
    ];

    for key in alternative_keys.iter() {
        if let Some(width_str) = tags.get(*key) {
            if let Some(width) = parse_width_to_blocks(width_str) {
                return width;
            }
        }
    }

    default
}

pub fn generate_waterways(editor: &mut WorldEditor, element: &ProcessedWay) {
    if let Some(waterway_type) = element.tags.get("waterway") {
        let (default_width, waterway_depth) = get_waterway_dimensions(waterway_type);
        let waterway_width = infer_width_from_tags(&element.tags, default_width);

        // Skip layers below the ground level
        if matches!(
            element.tags.get("layer").map(|s| s.as_str()),
            Some("-1") | Some("-2") | Some("-3")
        ) {
            return;
        }

        // Process consecutive node pairs to create waterways
        // Use windows(2) to avoid connecting last node back to first
        for nodes_pair in element.nodes.windows(2) {
            let prev_node = nodes_pair[0].xz();
            let current_node = nodes_pair[1].xz();

            // Draw a line between the current and previous node
            let bresenham_points: Vec<(i32, i32, i32)> = bresenham_line(
                prev_node.x,
                0,
                prev_node.z,
                current_node.x,
                0,
                current_node.z,
            );

            for (bx, _, bz) in bresenham_points {
                // Create water channel with proper depth and sloped banks
                create_water_channel(editor, bx, bz, waterway_width, waterway_depth);
            }
        }
    }
}

/// Determines width and depth based on waterway type
fn get_waterway_dimensions(waterway_type: &str) -> (i32, i32) {
    match waterway_type {
        "river" => (8, 3),    // Large rivers: 8 blocks wide, 3 blocks deep
        "canal" => (6, 2),    // Canals: 6 blocks wide, 2 blocks deep
        "stream" => (3, 2),   // Streams: 3 blocks wide, 2 blocks deep
        "fairway" => (12, 3), // Shipping fairways: 12 blocks wide, 3 blocks deep
        "flowline" => (2, 1), // Water flow lines: 2 blocks wide, 1 block deep
        "brook" => (2, 1),    // Small brooks: 2 blocks wide, 1 block deep
        "ditch" => (2, 1),    // Ditches: 2 blocks wide, 1 block deep
        "drain" => (1, 1),    // Drainage: 1 block wide, 1 block deep
        _ => (4, 2),          // Default: 4 blocks wide, 2 blocks deep
    }
}

/// Creates a water channel with proper depth and sloped banks
fn create_water_channel(
    editor: &mut WorldEditor,
    center_x: i32,
    center_z: i32,
    width: i32,
    depth: i32,
) {
    let half_width = width / 2;

    for x in (center_x - half_width - 1)..=(center_x + half_width + 1) {
        for z in (center_z - half_width - 1)..=(center_z + half_width + 1) {
            let dx = (x - center_x).abs();
            let dz = (z - center_z).abs();
            let distance_from_center = dx.max(dz);

            if distance_from_center <= half_width {
                // Main water channel
                for y in (1 - depth)..=0 {
                    editor.set_block(WATER, x, y, z, None, None);
                }

                // Place one layer of dirt below the water channel
                editor.set_block(DIRT, x, -depth, z, None, None);

                // Clear vegetation above the water
                editor.set_block(AIR, x, 1, z, Some(&[GRASS, WHEAT, CARROTS, POTATOES]), None);
            } else if distance_from_center == half_width + 1 && depth > 1 {
                // Create sloped banks (one block interval slopes)
                let slope_depth = (depth - 1).max(1);
                for y in (1 - slope_depth)..=0 {
                    if y == 0 {
                        // Surface level - place water or air
                        editor.set_block(WATER, x, y, z, None, None);
                    } else {
                        // Below surface - dig out for slope
                        editor.set_block(AIR, x, y, z, None, None);
                    }
                }

                // Place one layer of dirt below the sloped areas
                editor.set_block(DIRT, x, -slope_depth, z, None, None);

                // Clear vegetation above sloped areas
                editor.set_block(AIR, x, 1, z, Some(&[GRASS, WHEAT, CARROTS, POTATOES]), None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinate_system::cartesian::XZBBox;
    use crate::coordinate_system::geographic::LLBBox;
    use crate::osm_parser::ProcessedNode;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn build_way(tags: HashMap<String, String>, nodes: Vec<(i32, i32)>) -> ProcessedWay {
        let mut processed_nodes = Vec::new();
        for (i, (x, z)) in nodes.into_iter().enumerate() {
            processed_nodes.push(ProcessedNode {
                id: i as u64,
                tags: HashMap::new(),
                x,
                z,
            });
        }
        ProcessedWay {
            id: 1,
            nodes: processed_nodes,
            tags,
        }
    }

    #[test]
    fn width_tag_with_units_is_used() {
        let xzbbox = XZBBox::rect_from_xz_lengths(120.0, 120.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);
        let tags = HashMap::from([
            (String::from("waterway"), String::from("river")),
            (String::from("width"), String::from("30 m")),
        ]);
        let way = build_way(tags, vec![(50, 20), (50, 80)]);
        generate_waterways(&mut editor, &way);

        // width 30 -> half width 15, slopes at 16 -> ensure water within and not beyond
        assert!(editor.check_for_block(35, 0, 50, Some(&[WATER])));
        assert!(!editor.check_for_block(67, 0, 50, Some(&[WATER])));
    }

    #[test]
    fn infers_width_from_riverbank_metadata() {
        let xzbbox = XZBBox::rect_from_xz_lengths(120.0, 120.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);
        let tags = HashMap::from([
            (String::from("waterway"), String::from("river")),
            (String::from("riverbank:width"), String::from("40")),
        ]);
        let way = build_way(tags, vec![(60, 20), (60, 80)]);
        generate_waterways(&mut editor, &way);

        // width 40 -> half width 20, slopes at 21
        assert!(editor.check_for_block(40, 0, 50, Some(&[WATER])));
        assert!(!editor.check_for_block(82, 0, 50, Some(&[WATER])));
    }

    #[test]
    fn defaults_when_no_width_metadata() {
        let xzbbox = XZBBox::rect_from_xz_lengths(120.0, 120.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);
        let tags = HashMap::from([(String::from("waterway"), String::from("river"))]);
        let way = build_way(tags, vec![(70, 20), (70, 80)]);
        generate_waterways(&mut editor, &way);

        // default width 8 -> half 4, slopes at 5
        assert!(editor.check_for_block(66, 0, 50, Some(&[WATER])));
        assert!(!editor.check_for_block(64, 0, 50, Some(&[WATER])));
    }
}
