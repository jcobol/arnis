use geo::coords_iter::CoordsIter;
use geo::{BooleanOps, Contains, Coord, Intersects, LineString, Point, Polygon, Rect};
use std::collections::{HashMap, VecDeque};
use std::sync::Once;
use std::time::Instant;

use crate::bresenham::bresenham_line;

use crate::{
    biome_definitions::{self, Biome},
    biomes::biome_from_tags,
    block_definitions::WATER,
    coordinate_system::cartesian::XZPoint,
    osm_parser::{ProcessedMemberRole, ProcessedNode, ProcessedRelation, ProcessedWay},
    world_editor::WorldEditor,
};

static LOG_SAMPLE: Once = Once::new();

fn generate_water_areas_internal(
    editor: &mut WorldEditor,
    element: &ProcessedRelation,
    fill_outside: bool,
) {
    let start_time = Instant::now();
    let biome = biome_from_tags(&element.tags).unwrap_or(biome_definitions::PLAINS);

    if !fill_outside {
        let is_water = element.tags.contains_key("water")
            || element.tags.get("natural") == Some(&"water".to_string())
            || element.tags.get("waterway") == Some(&"riverbank".to_string());
        if !is_water {
            return;
        }
    }

    if let Some(layer) = element.tags.get("layer") {
        if layer.parse::<i32>().map(|x| x < 0).unwrap_or(false) {
            return;
        }
    }

    let mut outers: Vec<Vec<ProcessedNode>> = vec![];
    let mut inners: Vec<Vec<ProcessedNode>> = vec![];

    for mem in &element.members {
        match mem.role {
            ProcessedMemberRole::Outer => outers.push(mem.way.nodes.clone()),
            ProcessedMemberRole::Inner => inners.push(mem.way.nodes.clone()),
        }
    }

    let mut all_lines: Vec<Vec<ProcessedNode>> = Vec::new();
    all_lines.extend(outers.clone());
    all_lines.extend(inners.clone());
    let mut all_lines_open = false;
    for o in &outers {
        if o.first().map(|n| n.id) != o.last().map(|n| n.id) {
            all_lines_open = true;
            break;
        }
    }
    if all_lines_open {
        println!("barrier fill (inside) lines: {}", all_lines.len());
        let water_level = if let Some(g) = editor.get_ground() {
            if g.elevation_enabled {
                g.ground_level()
            } else {
                0
            }
        } else {
            0
        };
        fill_from_barriers(editor, &all_lines, false, water_level, biome);
        return;
    }

    for (i, outer_nodes) in outers.iter().enumerate() {
        let mut individual_outers = vec![outer_nodes.clone()];

        merge_loopy_loops(&mut individual_outers);
        if !verify_loopy_loops(&individual_outers) {
            println!(
                "Skipping invalid outer polygon {} for relation {}",
                i + 1,
                element.id
            );
            println!("barrier fill (inside) lines: {}", all_lines.len());
            let water_level = if let Some(g) = editor.get_ground() {
                if g.elevation_enabled {
                    g.ground_level()
                } else {
                    0
                }
            } else {
                0
            };
            fill_from_barriers(editor, &all_lines, false, water_level, biome);
            return;
        }

        merge_loopy_loops(&mut inners);
        if !verify_loopy_loops(&inners) {
            println!("barrier fill (inside) lines: {}", all_lines.len());
            let water_level = if let Some(g) = editor.get_ground() {
                if g.elevation_enabled {
                    g.ground_level()
                } else {
                    0
                }
            } else {
                0
            };
            fill_from_barriers(editor, &all_lines, false, water_level, biome);
            return;
        }

        let (min_x, min_z) = editor.get_min_coords();
        let (max_x, max_z) = editor.get_max_coords();

        // Clip assembled outer loops to world bounds. Clipping happens after
        // loop assembly to preserve closed geometry.
        let rect = Rect::new(
            Coord {
                x: min_x as f64,
                y: min_z as f64,
            },
            Coord {
                x: max_x as f64,
                y: max_z as f64,
            },
        );
        let mut clipped_outers: Vec<Vec<ProcessedNode>> = Vec::new();
        for outer in &individual_outers {
            let exterior: Vec<_> = outer.iter().map(|n| (n.x as f64, n.z as f64)).collect();
            let polygon = Polygon::new(LineString::from(exterior), vec![]);
            let clipped = polygon.intersection(&rect.to_polygon());
            for p in clipped {
                let nodes = p
                    .exterior()
                    .coords_iter()
                    .map(|c| ProcessedNode {
                        id: 0,
                        tags: HashMap::new(),
                        x: c.x.round() as i32,
                        z: c.y.round() as i32,
                    })
                    .collect::<Vec<_>>();
                clipped_outers.push(nodes);
            }
        }
        individual_outers = clipped_outers;

        let individual_outers_xz: Vec<Vec<XZPoint>> = individual_outers
            .iter()
            .map(|x| x.iter().map(|y| y.xz()).collect::<Vec<_>>())
            .collect();
        let inners_xz: Vec<Vec<XZPoint>> = inners
            .iter()
            .map(|x| x.iter().map(|y| y.xz()).collect::<Vec<_>>())
            .collect();

        let width = (max_x - min_x + 1) as usize;
        let height = (max_z - min_z + 1) as usize;
        let mut barrier = vec![vec![false; width]; height];
        let mut seals_added = 0;
        for outer in &individual_outers_xz {
            seals_added += rasterize_and_seal(outer, &mut barrier, min_x, min_z, max_x, max_z);
        }
        println!("water area seals added: {}", seals_added);

        let default_level = 0;
        let water_level = if let Some(ground) = editor.get_ground() {
            if ground.elevation_enabled {
                let outer_points = individual_outers_xz
                    .iter()
                    .flatten()
                    .map(|pt| XZPoint::new(pt.x - min_x, pt.z - min_z));
                ground.min_level(outer_points).unwrap_or(default_level)
            } else {
                default_level
            }
        } else {
            default_level
        };

        inverse_floodfill(
            min_x,
            min_z,
            max_x,
            max_z,
            individual_outers_xz,
            inners_xz,
            water_level,
            editor,
            start_time,
            fill_outside,
            biome,
        );
    }
}

pub fn generate_water_areas(editor: &mut WorldEditor, element: &ProcessedRelation) {
    generate_water_areas_internal(editor, element, false);
}

fn generate_water_area_from_way_internal(
    editor: &mut WorldEditor,
    way: &ProcessedWay,
    fill_outside: bool,
) {
    let start_time = Instant::now();
    let biome = biome_from_tags(&way.tags).unwrap_or(biome_definitions::PLAINS);

    if !fill_outside {
        let is_water = way.tags.contains_key("water")
            || way.tags.get("natural") == Some(&"water".to_string())
            || way.tags.get("waterway") == Some(&"riverbank".to_string())
            || way.tags.get("water") == Some(&"river".to_string())
            || (way.tags.get("waterway") == Some(&"river".to_string())
                && way.tags.get("area") == Some(&"yes".to_string()));
        if !is_water {
            return;
        }
    }

    if let Some(layer) = way.tags.get("layer") {
        if layer.parse::<i32>().map(|x| x < 0).unwrap_or(false) {
            return;
        }
    }

    if way.nodes.is_empty() {
        return;
    }

    if way.nodes.first().map(|n| n.id) != way.nodes.last().map(|n| n.id) {
        println!("barrier fill (inside) lines: 1");
        let water_level = if let Some(g) = editor.get_ground() {
            if g.elevation_enabled {
                g.ground_level()
            } else {
                0
            }
        } else {
            0
        };
        fill_from_barriers(
            editor,
            &[way.nodes.clone()],
            fill_outside,
            water_level,
            biome,
        );
        return;
    }

    let outer_xz: Vec<XZPoint> = way.nodes.iter().map(|n| n.xz()).collect();
    let (min_x, min_z) = editor.get_min_coords();
    let (max_x, max_z) = editor.get_max_coords();

    let default_level = 0;
    let water_level = if let Some(ground) = editor.get_ground() {
        if ground.elevation_enabled {
            let outer_points = outer_xz
                .iter()
                .map(|pt| XZPoint::new(pt.x - min_x, pt.z - min_z));
            ground.min_level(outer_points).unwrap_or(default_level)
        } else {
            default_level
        }
    } else {
        default_level
    };

    inverse_floodfill(
        min_x,
        min_z,
        max_x,
        max_z,
        vec![outer_xz],
        vec![],
        water_level,
        editor,
        start_time,
        fill_outside,
        biome,
    );
}

pub fn generate_water_area_from_way(editor: &mut WorldEditor, way: &ProcessedWay) {
    generate_water_area_from_way_internal(editor, way, false);
}
fn in_bounds(x: i32, z: i32, min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> bool {
    x >= min_x && x <= max_x && z >= min_z && z <= max_z
}

fn clip_to_border(
    a: (i32, i32),
    b: (i32, i32),
    min_x: i32,
    min_z: i32,
    max_x: i32,
    max_z: i32,
) -> (i32, i32) {
    let (ax, az) = (a.0 as f32, a.1 as f32);
    let (bx, bz) = (b.0 as f32, b.1 as f32);
    let dx = bx - ax;
    let dz = bz - az;
    let mut t_candidates: Vec<f32> = Vec::new();

    if dx != 0.0 {
        if bx < min_x as f32 {
            t_candidates.push((min_x as f32 - ax) / dx);
        } else if bx > max_x as f32 {
            t_candidates.push((max_x as f32 - ax) / dx);
        }
    }
    if dz != 0.0 {
        if bz < min_z as f32 {
            t_candidates.push((min_z as f32 - az) / dz);
        } else if bz > max_z as f32 {
            t_candidates.push((max_z as f32 - az) / dz);
        }
    }

    let t = t_candidates
        .into_iter()
        .filter(|t| *t >= 0.0 && *t <= 1.0)
        .fold(f32::INFINITY, f32::min);
    let x = ax + dx * t;
    let z = az + dz * t;
    let ix = x.round() as i32;
    let iz = z.round() as i32;
    (ix.clamp(min_x, max_x), iz.clamp(min_z, max_z))
}

fn draw_along_border(
    mut from: (i32, i32),
    to: (i32, i32),
    min_x: i32,
    min_z: i32,
    max_x: i32,
    max_z: i32,
    barrier: &mut [Vec<bool>],
) -> i32 {
    let width = (max_x - min_x + 1) as i32;
    let height = (max_z - min_z + 1) as i32;
    let perimeter = 2 * (width + height) - 4;

    let idx = |x: i32, z: i32| -> i32 {
        if z == min_z {
            x - min_x
        } else if x == max_x {
            (max_x - min_x) + (z - min_z)
        } else if z == max_z {
            (max_x - min_x) + (max_z - min_z) + (max_x - x)
        } else {
            2 * (max_x - min_x) + (max_z - min_z) + (max_z - z)
        }
    };

    let mut idx_from = idx(from.0, from.1);
    let idx_to = idx(to.0, to.1);
    let cw_dist = (idx_to - idx_from + perimeter) % perimeter;
    let ccw_dist = (idx_from - idx_to + perimeter) % perimeter;
    let clockwise = cw_dist <= ccw_dist;
    let steps = if clockwise { cw_dist } else { ccw_dist };
    let mut seals = 0;

    for _ in 0..=steps {
        let gx = (from.0 - min_x) as usize;
        let gz = (from.1 - min_z) as usize;
        if !barrier[gz][gx] {
            barrier[gz][gx] = true;
            seals += 1;
        }

        if from == to {
            break;
        }

        if clockwise {
            if from.1 == min_z && from.0 < max_x {
                from.0 += 1;
            } else if from.0 == max_x && from.1 < max_z {
                from.1 += 1;
            } else if from.1 == max_z && from.0 > min_x {
                from.0 -= 1;
            } else if from.0 == min_x && from.1 > min_z {
                from.1 -= 1;
            }
        } else {
            if from.1 == min_z && from.0 > min_x {
                from.0 -= 1;
            } else if from.0 == min_x && from.1 < max_z {
                from.1 += 1;
            } else if from.1 == max_z && from.0 < max_x {
                from.0 += 1;
            } else if from.0 == max_x && from.1 > min_z {
                from.1 -= 1;
            }
        }

        idx_from = if clockwise {
            (idx_from + 1) % perimeter
        } else {
            (idx_from - 1 + perimeter) % perimeter
        };
    }

    seals
}

fn rasterize_and_seal(
    line: &[XZPoint],
    barrier: &mut [Vec<bool>],
    min_x: i32,
    min_z: i32,
    max_x: i32,
    max_z: i32,
) -> i32 {
    let mut border_nodes: Vec<(i32, i32)> = Vec::new();
    let mut seals_added = 0;

    let mut inside_prev = line
        .first()
        .map(|n| in_bounds(n.x, n.z, min_x, min_z, max_x, max_z))
        .unwrap_or(false);

    for pair in line.windows(2) {
        let a = &pair[0];
        let b = &pair[1];
        let inside_curr = in_bounds(b.x, b.z, min_x, min_z, max_x, max_z);

        for (x, _, z) in bresenham_line(a.x, 0, a.z, b.x, 0, b.z) {
            if x < min_x || x > max_x || z < min_z || z > max_z {
                continue;
            }
            let gx = (x - min_x) as usize;
            let gz = (z - min_z) as usize;
            barrier[gz][gx] = true;
        }

        if inside_prev != inside_curr {
            let clipped = clip_to_border((a.x, a.z), (b.x, b.z), min_x, min_z, max_x, max_z);
            border_nodes.push(clipped);
        }
        inside_prev = inside_curr;
    }

    if border_nodes.len() % 2 != 0 {
        println!(
            "odd number of border intersections for way: {}",
            border_nodes.len()
        );
        return seals_added;
    }

    for pair in border_nodes.chunks(2) {
        seals_added += draw_along_border(pair[0], pair[1], min_x, min_z, max_x, max_z, barrier);
    }

    seals_added
}

fn fill_from_barriers(
    editor: &mut WorldEditor,
    lines: &[Vec<ProcessedNode>],
    fill_outside: bool,
    water_level: i32,
    biome: Biome,
) {
    let (min_x, min_z) = editor.get_min_coords();
    let (max_x, max_z) = editor.get_max_coords();
    let width = (max_x - min_x + 1) as usize;
    let height = (max_z - min_z + 1) as usize;

    let mut barrier = vec![vec![false; width]; height];
    let mut seals_added_count = 0;

    for way in lines {
        let line: Vec<XZPoint> = way.iter().map(|n| n.xz()).collect();
        seals_added_count += rasterize_and_seal(&line, &mut barrier, min_x, min_z, max_x, max_z);
    }
    println!("barrier seals added: {}", seals_added_count);

    let mut outside = vec![vec![false; width]; height];
    let mut q: VecDeque<(i32, i32)> = VecDeque::new();

    for x in 0..width {
        if !barrier[0][x] {
            q.push_back((x as i32, 0));
        }
        if !barrier[height - 1][x] {
            q.push_back((x as i32, (height - 1) as i32));
        }
    }
    for z in 0..height {
        if !barrier[z][0] {
            q.push_back((0, z as i32));
        }
        if !barrier[z][width - 1] {
            q.push_back(((width - 1) as i32, z as i32));
        }
    }

    while let Some((x, z)) = q.pop_front() {
        if x < 0 || z < 0 || x >= width as i32 || z >= height as i32 {
            continue;
        }
        let ux = x as usize;
        let uz = z as usize;
        if outside[uz][ux] || barrier[uz][ux] {
            continue;
        }
        outside[uz][ux] = true;
        q.push_back((x - 1, z));
        q.push_back((x + 1, z));
        q.push_back((x, z - 1));
        q.push_back((x, z + 1));
    }

    let ground = editor.get_ground().cloned();

    for z in 0..height {
        for x in 0..width {
            let fill = if fill_outside {
                outside[z][x] || barrier[z][x]
            } else {
                !outside[z][x] && !barrier[z][x]
            };
            if fill {
                let world_x = min_x + x as i32;
                let world_z = min_z + z as i32;
                if let Some(ref g) = ground {
                    let terrain = g.level(XZPoint::new(world_x - min_x, world_z - min_z));
                    if terrain >= water_level {
                        LOG_SAMPLE.call_once(|| {
                            println!(
                                "sample column ({}, {}): terrain={}, water_level={}",
                                world_x, world_z, terrain, water_level
                            );
                        });
                        for y in water_level..=terrain {
                            editor.set_block_absolute(WATER, world_x, y, world_z, None, Some(&[]));
                            editor.set_biome_absolute(biome, world_x, y, world_z);
                        }
                    } else {
                        editor.set_block_absolute(
                            WATER,
                            world_x,
                            water_level,
                            world_z,
                            None,
                            Some(&[]),
                        );
                        editor.set_biome_absolute(biome, world_x, water_level, world_z);
                    }
                } else {
                    editor.set_block_absolute(
                        WATER,
                        world_x,
                        water_level,
                        world_z,
                        None,
                        Some(&[]),
                    );
                    editor.set_biome_absolute(biome, world_x, water_level, world_z);
                }
            }
        }
    }
}

pub fn generate_coastlines(editor: &mut WorldEditor, ways: &[Vec<ProcessedNode>]) {
    if ways.is_empty() {
        return;
    }
    println!("coastline segments: {}", ways.len());
    let level = if let Some(g) = editor.get_ground() {
        if g.elevation_enabled {
            g.ground_level()
        } else {
            0
        }
    } else {
        0
    };
    fill_from_barriers(editor, ways, true, level, biome_definitions::OCEAN);
}

// Merges ways that share nodes into full loops
fn merge_loopy_loops(loops: &mut Vec<Vec<ProcessedNode>>) {
    let mut removed: Vec<usize> = vec![];
    let mut merged: Vec<Vec<ProcessedNode>> = vec![];

    for i in 0..loops.len() {
        for j in 0..loops.len() {
            if i == j {
                continue;
            }

            if removed.contains(&i) || removed.contains(&j) {
                continue;
            }

            let x: &Vec<ProcessedNode> = &loops[i];
            let y: &Vec<ProcessedNode> = &loops[j];

            // it's looped already
            if x[0].id == x.last().unwrap().id {
                continue;
            }

            // it's looped already
            if y[0].id == y.last().unwrap().id {
                continue;
            }

            if x[0].id == y[0].id {
                removed.push(i);
                removed.push(j);

                let mut x: Vec<ProcessedNode> = x.clone();
                x.reverse();
                x.extend(y.iter().skip(1).cloned());
                merged.push(x);
            } else if x.last().unwrap().id == y.last().unwrap().id {
                removed.push(i);
                removed.push(j);

                let mut x: Vec<ProcessedNode> = x.clone();
                x.extend(y.iter().rev().skip(1).cloned());

                merged.push(x);
            } else if x[0].id == y.last().unwrap().id {
                removed.push(i);
                removed.push(j);

                let mut y: Vec<ProcessedNode> = y.clone();
                y.extend(x.iter().skip(1).cloned());

                merged.push(y);
            } else if x.last().unwrap().id == y[0].id {
                removed.push(i);
                removed.push(j);

                let mut x: Vec<ProcessedNode> = x.clone();
                x.extend(y.iter().skip(1).cloned());

                merged.push(x);
            }
        }
    }

    removed.sort();

    for r in removed.iter().rev() {
        loops.remove(*r);
    }

    let merged_len: usize = merged.len();
    for m in merged {
        loops.push(m);
    }

    if merged_len > 0 {
        merge_loopy_loops(loops);
    }
}

fn verify_loopy_loops(loops: &[Vec<ProcessedNode>]) -> bool {
    let mut valid: bool = true;
    for l in loops {
        if l[0].id != l.last().unwrap().id {
            eprintln!("WARN: Disconnected loop");
            valid = false;
        }
    }

    valid
}

// Water areas are absolutely huge. We can't easily flood fill the entire thing.
// Instead, we'll iterate over all the blocks in our MC world, and check if each
// one is in the river or not
#[allow(clippy::too_many_arguments)]
fn inverse_floodfill(
    min_x: i32,
    min_z: i32,
    max_x: i32,
    max_z: i32,
    outers: Vec<Vec<XZPoint>>,
    inners: Vec<Vec<XZPoint>>,
    water_level: i32,
    editor: &mut WorldEditor,
    start_time: Instant,
    fill_outside: bool,
    biome: Biome,
) {
    let inners: Vec<_> = inners
        .into_iter()
        .map(|x| {
            Polygon::new(
                LineString::from(
                    x.iter()
                        .map(|pt| (pt.x as f64, pt.z as f64))
                        .collect::<Vec<_>>(),
                ),
                vec![],
            )
        })
        .collect();

    let outers: Vec<_> = outers
        .into_iter()
        .map(|x| {
            Polygon::new(
                LineString::from(
                    x.iter()
                        .map(|pt| (pt.x as f64, pt.z as f64))
                        .collect::<Vec<_>>(),
                ),
                vec![],
            )
        })
        .collect();

    inverse_floodfill_recursive(
        (min_x, min_z),
        (max_x, max_z),
        &outers,
        &inners,
        water_level,
        editor,
        start_time,
        fill_outside,
        biome,
    );
}

fn inverse_floodfill_recursive(
    min: (i32, i32),
    max: (i32, i32),
    outers: &[Polygon],
    inners: &[Polygon],
    water_level: i32,
    editor: &mut WorldEditor,
    start_time: Instant,
    fill_outside: bool,
    biome: Biome,
) {
    // Check if we've exceeded 25 seconds
    if start_time.elapsed().as_secs() > 25 {
        // Fall back: brute-force fill for the remaining region so we never leave it empty.
        inverse_floodfill_iterative(
            min,
            max,
            water_level,
            outers,
            inners,
            editor,
            fill_outside,
            biome,
        );
        return;
    }

    const ITERATIVE_THRES: i64 = 10_000;

    if min.0 > max.0 || min.1 > max.1 {
        return;
    }

    // Multiply as i64 to avoid overflow; in release builds where unchecked math is
    // enabled, this could cause the rest of this code to end up in an infinite loop.
    if ((max.0 - min.0) as i64) * ((max.1 - min.1) as i64) < ITERATIVE_THRES {
        inverse_floodfill_iterative(
            min,
            max,
            water_level,
            outers,
            inners,
            editor,
            fill_outside,
            biome,
        );
        return;
    }

    let center_x: i32 = (min.0 + max.0) / 2;
    let center_z: i32 = (min.1 + max.1) / 2;
    let quadrants: [(i32, i32, i32, i32); 4] = [
        (min.0, center_x, min.1, center_z),
        (center_x, max.0, min.1, center_z),
        (min.0, center_x, center_z, max.1),
        (center_x, max.0, center_z, max.1),
    ];

    for (min_x, max_x, min_z, max_z) in quadrants {
        let rect: Rect = Rect::new(
            Point::new(min_x as f64, min_z as f64),
            Point::new(max_x as f64, max_z as f64),
        );

        let outers_intersects: Vec<_> = outers
            .iter()
            .filter(|poly| poly.intersects(&rect))
            .cloned()
            .collect();
        let inners_intersects: Vec<_> = inners
            .iter()
            .filter(|poly| poly.intersects(&rect))
            .cloned()
            .collect();

        let inside =
            outers.iter().any(|outer| outer.contains(&rect)) && inners_intersects.is_empty();

        if (!fill_outside && inside)
            || (fill_outside
                && !inside
                && outers_intersects.is_empty()
                && inners_intersects.is_empty())
        {
            rect_fill(min_x, max_x, min_z, max_z, water_level, editor, biome);
            continue;
        }

        if !outers_intersects.is_empty() || !inners_intersects.is_empty() {
            inverse_floodfill_recursive(
                (min_x, min_z),
                (max_x, max_z),
                &outers_intersects,
                &inners_intersects,
                water_level,
                editor,
                start_time,
                fill_outside,
                biome,
            );
        }
    }
}

// once we "zoom in" enough, it's more efficient to switch to iteration
fn inverse_floodfill_iterative(
    min: (i32, i32),
    max: (i32, i32),
    water_level: i32,
    outers: &[Polygon],
    inners: &[Polygon],
    editor: &mut WorldEditor,
    fill_outside: bool,
    biome: Biome,
) {
    let ground = editor.get_ground().cloned();
    let (min_x, min_z) = editor.get_min_coords();
    for x in min.0..max.0 {
        for z in min.1..max.1 {
            let cell = Rect::new(
                Point::new(x as f64, z as f64),
                Point::new((x + 1) as f64, (z + 1) as f64),
            );

            let in_outer = outers.iter().any(|poly| poly.intersects(&cell));
            let in_inner = inners.iter().any(|poly| poly.intersects(&cell));

            if (fill_outside && (!in_outer || in_inner)) || (!fill_outside && in_outer && !in_inner)
            {
                if let Some(ref g) = ground {
                    let terrain = g.level(XZPoint::new(x - min_x, z - min_z));
                    if terrain >= water_level {
                        LOG_SAMPLE.call_once(|| {
                            println!(
                                "sample column ({}, {}): terrain={}, water_level={}",
                                x, z, terrain, water_level
                            );
                        });
                        for y in water_level..=terrain {
                            editor.set_block_absolute(WATER, x, y, z, None, Some(&[]));
                            editor.set_biome_absolute(biome, x, y, z);
                        }
                    } else {
                        editor.set_block_absolute(WATER, x, water_level, z, None, Some(&[]));
                        editor.set_biome_absolute(biome, x, water_level, z);
                    }
                } else {
                    editor.set_block_absolute(WATER, x, water_level, z, None, Some(&[]));
                    editor.set_biome_absolute(biome, x, water_level, z);
                }
            }
        }
    }
}

fn rect_fill(
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
    water_level: i32,
    editor: &mut WorldEditor,
    biome: Biome,
) {
    let ground = editor.get_ground().cloned();
    let (min_x_world, min_z_world) = editor.get_min_coords();
    for x in min_x..max_x {
        for z in min_z..max_z {
            if let Some(ref g) = ground {
                let terrain = g.level(XZPoint::new(x - min_x_world, z - min_z_world));
                if terrain >= water_level {
                    LOG_SAMPLE.call_once(|| {
                        println!(
                            "sample column ({}, {}): terrain={}, water_level={}",
                            x, z, terrain, water_level
                        );
                    });
                    for y in water_level..=terrain {
                        editor.set_block_absolute(WATER, x, y, z, None, Some(&[]));
                        editor.set_biome_absolute(biome, x, y, z);
                    }
                } else {
                    editor.set_block_absolute(WATER, x, water_level, z, None, Some(&[]));
                    editor.set_biome_absolute(biome, x, water_level, z);
                }
            } else {
                editor.set_block_absolute(WATER, x, water_level, z, None, Some(&[]));
                editor.set_biome_absolute(biome, x, water_level, z);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_definitions::{DIRT, WATER};
    use crate::coordinate_system::{
        cartesian::{XZBBox, XZPoint},
        geographic::LLBBox,
    };
    use crate::ground::Ground;
    use crate::osm_parser::{
        ProcessedMember, ProcessedMemberRole, ProcessedRelation, ProcessedWay,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn riverbank_relation_places_water() {
        let xzbbox = XZBBox::rect_from_xz_lengths(20.0, 20.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);

        let n1 = ProcessedNode {
            id: 1,
            tags: HashMap::new(),
            x: 0,
            z: 0,
        };
        let n2 = ProcessedNode {
            id: 2,
            tags: HashMap::new(),
            x: 10,
            z: 0,
        };
        let n3 = ProcessedNode {
            id: 3,
            tags: HashMap::new(),
            x: 10,
            z: 10,
        };
        let n4 = ProcessedNode {
            id: 4,
            tags: HashMap::new(),
            x: 0,
            z: 10,
        };
        let outer = vec![n1.clone(), n2.clone(), n3.clone(), n4.clone(), n1.clone()];

        let way = ProcessedWay {
            id: 1,
            nodes: outer,
            tags: HashMap::new(),
        };
        let member = ProcessedMember {
            role: ProcessedMemberRole::Outer,
            way,
        };
        let relation = ProcessedRelation {
            id: 1,
            tags: HashMap::from([(String::from("waterway"), String::from("riverbank"))]),
            members: vec![member],
        };

        generate_water_areas(&mut editor, &relation);

        for x in 1..10 {
            for z in 1..10 {
                assert!(editor.check_for_block(x, 0, z, Some(&[WATER])));
            }
        }
    }

    #[test]
    fn lake_way_places_water() {
        let xzbbox = XZBBox::rect_from_xz_lengths(20.0, 20.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);

        let n1 = ProcessedNode {
            id: 1,
            tags: HashMap::new(),
            x: 0,
            z: 0,
        };
        let n2 = ProcessedNode {
            id: 2,
            tags: HashMap::new(),
            x: 10,
            z: 0,
        };
        let n3 = ProcessedNode {
            id: 3,
            tags: HashMap::new(),
            x: 10,
            z: 10,
        };
        let n4 = ProcessedNode {
            id: 4,
            tags: HashMap::new(),
            x: 0,
            z: 10,
        };
        let nodes = vec![n1.clone(), n2.clone(), n3.clone(), n4.clone(), n1.clone()];

        let way = ProcessedWay {
            id: 1,
            nodes,
            tags: HashMap::from([
                (String::from("natural"), String::from("water")),
                (String::from("water"), String::from("reservoir")),
            ]),
        };

        generate_water_area_from_way(&mut editor, &way);

        for x in 1..10 {
            for z in 1..10 {
                assert!(editor.check_for_block(x, 0, z, Some(&[WATER])));
            }
        }
    }

    #[test]
    fn water_area_excavates_to_min_level() {
        let xzbbox = XZBBox::rect_from_xz_lengths(20.0, 20.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);

        // Create artificial ground with varying heights
        let mut heights = vec![vec![5; 20]; 20];
        for row in heights.iter_mut() {
            for x in 10..20 {
                row[x] = 3;
            }
        }
        let ground = Ground::from_heights(0, heights.clone());
        editor.set_ground(&ground);

        // Pre-fill terrain blocks up to ground level
        for x in 0..20 {
            for z in 0..20 {
                let terrain = ground.level(XZPoint::new(x, z));
                for y in 0..=terrain {
                    editor.set_block_absolute(DIRT, x as i32, y, z as i32, None, None);
                }
            }
        }

        // Square polygon covering entire area
        let n1 = ProcessedNode {
            id: 1,
            tags: HashMap::new(),
            x: 0,
            z: 0,
        };
        let n2 = ProcessedNode {
            id: 2,
            tags: HashMap::new(),
            x: 19,
            z: 0,
        };
        let n3 = ProcessedNode {
            id: 3,
            tags: HashMap::new(),
            x: 19,
            z: 19,
        };
        let n4 = ProcessedNode {
            id: 4,
            tags: HashMap::new(),
            x: 0,
            z: 19,
        };
        let outer = vec![n1.clone(), n2.clone(), n3.clone(), n4.clone(), n1.clone()];

        let way = ProcessedWay {
            id: 1,
            tags: HashMap::new(),
            nodes: outer.clone(),
        };
        let member = ProcessedMember {
            role: ProcessedMemberRole::Outer,
            way,
        };
        let relation = ProcessedRelation {
            id: 1,
            tags: HashMap::from([(String::from("waterway"), String::from("riverbank"))]),
            members: vec![member],
        };

        generate_water_areas(&mut editor, &relation);

        // Water level should be min height (3)
        for x in 1..19 {
            for z in 1..19 {
                assert_eq!(
                    editor.get_block_absolute(x, 3, z),
                    Some(WATER),
                    "x {x} z {z}"
                );
                if x < 10 {
                    // Higher terrain should be filled with water up to the surface
                    assert_eq!(
                        editor.get_block_absolute(x, 4, z),
                        Some(WATER),
                        "x {x} z {z}"
                    );
                    assert_eq!(
                        editor.get_block_absolute(x, 5, z),
                        Some(WATER),
                        "x {x} z {z}"
                    );
                }
            }
        }
    }

    #[test]
    fn coastline_relation_fills_outside() {
        let xzbbox = XZBBox::rect_from_xz_lengths(10.0, 10.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(PathBuf::from("test_world"), &xzbbox, llbbox);

        let n1 = ProcessedNode {
            id: 1,
            tags: HashMap::new(),
            x: 2,
            z: 2,
        };
        let n2 = ProcessedNode {
            id: 2,
            tags: HashMap::new(),
            x: 8,
            z: 2,
        };
        let n3 = ProcessedNode {
            id: 3,
            tags: HashMap::new(),
            x: 8,
            z: 8,
        };
        let n4 = ProcessedNode {
            id: 4,
            tags: HashMap::new(),
            x: 2,
            z: 8,
        };
        let way_nodes = vec![n1.clone(), n2.clone(), n3.clone(), n4.clone(), n1.clone()];

        generate_coastlines(&mut editor, &[way_nodes]);

        assert!(editor.check_for_block(0, 0, 0, Some(&[WATER])));
        assert!(editor.check_for_block(9, 0, 9, Some(&[WATER])));
        assert!(!editor.check_for_block(5, 0, 5, Some(&[WATER])));
    }
}
