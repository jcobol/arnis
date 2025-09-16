use crate::block_definitions::*;
use crate::bresenham::bresenham_line;
use crate::osm_parser::ProcessedWay;
use crate::world_editor::WorldEditor;
use fastnbt::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RailShape {
    NorthSouth,
    EastWest,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
    AscendingEast,
    AscendingWest,
    AscendingNorth,
    AscendingSouth,
}

impl RailShape {
    fn as_str(&self) -> &'static str {
        match self {
            RailShape::NorthSouth => "north_south",
            RailShape::EastWest => "east_west",
            RailShape::NorthEast => "north_east",
            RailShape::NorthWest => "north_west",
            RailShape::SouthEast => "south_east",
            RailShape::SouthWest => "south_west",
            RailShape::AscendingEast => "ascending_east",
            RailShape::AscendingWest => "ascending_west",
            RailShape::AscendingNorth => "ascending_north",
            RailShape::AscendingSouth => "ascending_south",
        }
    }

    fn is_straight_or_ascending(&self) -> bool {
        matches!(
            self,
            RailShape::NorthSouth
                | RailShape::EastWest
                | RailShape::AscendingEast
                | RailShape::AscendingWest
                | RailShape::AscendingNorth
                | RailShape::AscendingSouth
        )
    }
}

pub fn generate_railways(editor: &mut WorldEditor, element: &ProcessedWay) {
    if let Some(railway_type) = element.tags.get("railway") {
        if [
            "proposed",
            "abandoned",
            "subway",
            "construction",
            "razed",
            "turntable",
        ]
        .contains(&railway_type.as_str())
        {
            return;
        }

        if let Some(subway) = element.tags.get("subway") {
            if subway == "yes" {
                return;
            }
        }

        if let Some(tunnel) = element.tags.get("tunnel") {
            if tunnel == "yes" {
                return;
            }
        }

        // Collect every point along the way into a single list so each
        // rail can see both its predecessor and successor, even across node
        // boundaries.
        let mut path_points: Vec<(i32, i32, i32)> = Vec::new();

        for i in 1..element.nodes.len() {
            let prev_node = element.nodes[i - 1].xz();
            let cur_node = element.nodes[i].xz();

            let points = bresenham_line(prev_node.x, 0, prev_node.z, cur_node.x, 0, cur_node.z);
            let smoothed_points = smooth_diagonal_rails(&points);

            if path_points.is_empty() {
                path_points.extend(smoothed_points);
            } else {
                path_points.extend(smoothed_points.into_iter().skip(1));
            }
        }

        if path_points.is_empty() {
            return;
        }

        // Track the ground height under each rail so we can keep corners level.
        let mut base_heights: Vec<i32> = path_points
            .iter()
            .map(|&(x, _, z)| editor.get_absolute_y(x, 0, z))
            .collect();

        for j in 1..path_points.len().saturating_sub(1) {
            let (cx, _, cz) = path_points[j];
            let (px, _, pz) = path_points[j - 1];
            let (nx, _, nz) = path_points[j + 1];

            let dir_prev = (cx - px, cz - pz);
            let dir_next = (nx - cx, nz - cz);

            // When the route turns, force the neighbours to share the same
            // base height as the corner. Without this flat run the game
            // cannot form a turn-and-climb transition.
            if dir_prev != dir_next {
                let current_height = base_heights[j];

                if base_heights[j + 1] > current_height {
                    base_heights[j + 1] = current_height;
                }

                if base_heights[j - 1] > current_height {
                    base_heights[j - 1] = current_height;
                }
            }
        }

        let mut rail_counter = 0;

        for (idx, (bx, _, bz)) in path_points.iter().enumerate() {
            let base_y = base_heights[idx];
            let rail_y = base_y + 1;

            // Rebuild the foundation and clear headroom using absolute
            // coordinates, which also overwrites whatever block the rail was
            // sitting on (slabs, planks, etc.).
            editor.set_block_absolute(GRAVEL, *bx, base_y, *bz, None, Some(&[]));
            editor.set_block_absolute(AIR, *bx, rail_y, *bz, None, Some(&[]));
            editor.set_block_absolute(AIR, *bx, rail_y + 1, *bz, None, Some(&[]));

            let prev = if idx > 0 {
                let (px, _, pz) = path_points[idx - 1];
                Some(((px, pz), base_heights[idx - 1] + 1))
            } else {
                None
            };
            let next = if idx + 1 < path_points.len() {
                let (nx, _, nz) = path_points[idx + 1];
                Some(((nx, nz), base_heights[idx + 1] + 1))
            } else {
                None
            };

            let rail_shape = determine_rail_shape((*bx, *bz), rail_y, prev, next);

            if rail_counter % 8 == 7 && rail_shape.is_straight_or_ascending() {
                let shape = rail_shape.as_str();
                let properties = Value::Compound(HashMap::from([
                    ("shape".to_string(), Value::String(shape.to_string())),
                    ("powered".to_string(), Value::String("true".to_string())),
                ]));
                editor.set_block_absolute(REDSTONE_BLOCK, *bx, base_y, *bz, None, Some(&[]));
                editor.set_block_with_properties_absolute(
                    BlockWithProperties::new(POWERED_RAIL, Some(properties)),
                    *bx,
                    rail_y,
                    *bz,
                    None,
                    Some(&[]),
                );
            } else {
                let shape = rail_shape.as_str();
                let properties = Value::Compound(HashMap::from([(
                    "shape".to_string(),
                    Value::String(shape.to_string()),
                )]));
                editor.set_block_with_properties_absolute(
                    BlockWithProperties::new(RAIL, Some(properties)),
                    *bx,
                    rail_y,
                    *bz,
                    None,
                    Some(&[]),
                );
                if rail_counter % 4 == 0 {
                    editor.set_block_absolute(OAK_LOG, *bx, base_y, *bz, None, Some(&[]));
                }
            }

            rail_counter += 1;
        }
    }
}

fn smooth_diagonal_rails(points: &[(i32, i32, i32)]) -> Vec<(i32, i32, i32)> {
    let mut smoothed = Vec::new();

    for i in 0..points.len() {
        let current = points[i];
        smoothed.push(current);

        if i + 1 >= points.len() {
            continue;
        }

        let next = points[i + 1];
        let (x1, y1, z1) = current;
        let (x2, _, z2) = next;

        // If points are diagonally adjacent
        if (x2 - x1).abs() == 1 && (z2 - z1).abs() == 1 {
            // Look ahead to determine best intermediate point
            let look_ahead = if i + 2 < points.len() {
                Some(points[i + 2])
            } else {
                None
            };

            // Look behind
            let look_behind = if i > 0 { Some(points[i - 1]) } else { None };

            // Choose intermediate point based on the overall curve direction
            let intermediate = if let Some((prev_x, _, _prev_z)) = look_behind {
                if prev_x == x1 {
                    // Coming from vertical, keep x constant
                    (x1, y1, z2)
                } else {
                    // Coming from horizontal, keep z constant
                    (x2, y1, z1)
                }
            } else if let Some((next_x, _, _next_z)) = look_ahead {
                if next_x == x2 {
                    // Going to vertical, keep x constant
                    (x2, y1, z1)
                } else {
                    // Going to horizontal, keep z constant
                    (x1, y1, z2)
                }
            } else {
                // Default to horizontal first if no context
                (x2, y1, z1)
            };

            smoothed.push(intermediate);
        }
    }

    smoothed
}

fn determine_rail_shape(
    current: (i32, i32),
    current_y: i32,
    prev: Option<((i32, i32), i32)>,
    next: Option<((i32, i32), i32)>,
) -> RailShape {
    let (x, z) = current;

    if let Some(&((px, pz), py)) = prev.as_ref() {
        if py > current_y {
            if let Some(shape) = ascending_shape_from_direction(px - x, pz - z) {
                return shape;
            }
        }
    }

    if let Some(&((nx, nz), ny)) = next.as_ref() {
        if ny > current_y {
            if let Some(shape) = ascending_shape_from_direction(nx - x, nz - z) {
                return shape;
            }
        }
    }

    let prev_pos = prev.map(|(pos, _)| pos);
    let next_pos = next.map(|(pos, _)| pos);

    match (prev_pos, next_pos) {
        (Some((px, pz)), Some((nx, nz))) => {
            if px == nx {
                RailShape::NorthSouth
            } else if pz == nz {
                RailShape::EastWest
            } else {
                // Calculate relative movements
                let from_prev = (px - x, pz - z);
                let to_next = (nx - x, nz - z);

                match (from_prev, to_next) {
                    // East to North or North to East
                    ((-1, 0), (0, -1)) | ((0, -1), (-1, 0)) => RailShape::NorthWest,
                    // West to North or North to West
                    ((1, 0), (0, -1)) | ((0, -1), (1, 0)) => RailShape::NorthEast,
                    // East to South or South to East
                    ((-1, 0), (0, 1)) | ((0, 1), (-1, 0)) => RailShape::SouthWest,
                    // West to South or South to West
                    ((1, 0), (0, 1)) | ((0, 1), (1, 0)) => RailShape::SouthEast,
                    _ => {
                        if (px - x).abs() > (pz - z).abs() {
                            RailShape::EastWest
                        } else {
                            RailShape::NorthSouth
                        }
                    }
                }
            }
        }
        (Some((px, pz)), None) | (None, Some((px, pz))) => {
            if px == x {
                RailShape::NorthSouth
            } else if pz == z {
                RailShape::EastWest
            } else {
                RailShape::NorthSouth
            }
        }
        (None, None) => RailShape::NorthSouth,
    }
}

fn ascending_shape_from_direction(dx: i32, dz: i32) -> Option<RailShape> {
    match (dx, dz) {
        (1, 0) => Some(RailShape::AscendingEast),
        (-1, 0) => Some(RailShape::AscendingWest),
        (0, 1) => Some(RailShape::AscendingSouth),
        (0, -1) => Some(RailShape::AscendingNorth),
        _ => None,
    }
}

pub fn generate_roller_coaster(editor: &mut WorldEditor, element: &ProcessedWay) {
    if let Some(roller_coaster) = element.tags.get("roller_coaster") {
        if roller_coaster == "track" {
            // Check if it's indoor (skip if yes)
            if let Some(indoor) = element.tags.get("indoor") {
                if indoor == "yes" {
                    return;
                }
            }

            // Check if layer is negative (skip if yes)
            if let Some(layer) = element.tags.get("layer") {
                if let Ok(layer_value) = layer.parse::<i32>() {
                    if layer_value < 0 {
                        return;
                    }
                }
            }

            let elevation_height = 4; // 4 blocks in the air
            let pillar_interval = 6; // Support pillars every 6 blocks

            // Same smoothing approach as the ground rails: build a merged
            // list of points so corners know their neighbours.
            let mut path_points: Vec<(i32, i32, i32)> = Vec::new();

            for i in 1..element.nodes.len() {
                let prev_node = element.nodes[i - 1].xz();
                let cur_node = element.nodes[i].xz();

                let points = bresenham_line(prev_node.x, 0, prev_node.z, cur_node.x, 0, cur_node.z);
                let smoothed_points = smooth_diagonal_rails(&points);

                if path_points.is_empty() {
                    path_points.extend(smoothed_points);
                } else {
                    path_points.extend(smoothed_points.into_iter().skip(1));
                }
            }

            if path_points.is_empty() {
                return;
            }

            for (idx, (bx, _, bz)) in path_points.iter().enumerate() {
                // Place track foundation at elevation height
                editor.set_block(IRON_BLOCK, *bx, elevation_height, *bz, None, None);

                let rail_y = elevation_height + 1;

                let prev = if idx > 0 {
                    let (px, _, pz) = path_points[idx - 1];
                    Some(((px, pz), rail_y))
                } else {
                    None
                };
                let next = if idx + 1 < path_points.len() {
                    let (nx, _, nz) = path_points[idx + 1];
                    Some(((nx, nz), rail_y))
                } else {
                    None
                };

                let rail_shape = determine_rail_shape((*bx, *bz), rail_y, prev, next);

                // Place rail on top of the foundation
                let properties = Value::Compound(HashMap::from([(
                    "shape".to_string(),
                    Value::String(rail_shape.as_str().to_string()),
                )]));
                editor.set_block_with_properties(
                    BlockWithProperties::new(RAIL, Some(properties)),
                    *bx,
                    rail_y,
                    *bz,
                    None,
                    None,
                );

                // Place support pillars every pillar_interval blocks
                if *bx % pillar_interval == 0 && *bz % pillar_interval == 0 {
                    // Create a pillar from ground level up to the track
                    for y in 1..elevation_height {
                        editor.set_block(IRON_BLOCK, *bx, y, *bz, None, None);
                    }
                }
            }
        }
    }
}
