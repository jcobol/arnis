use geo::{Contains, Intersects, LineString, Point, Polygon, Rect};
use std::collections::VecDeque;
use std::time::Instant;

use crate::bresenham::bresenham_line;

use crate::{
    block_definitions::{AIR, WATER},
    coordinate_system::cartesian::XZPoint,
    osm_parser::{ProcessedMemberRole, ProcessedNode, ProcessedRelation, ProcessedWay},
    world_editor::WorldEditor,
};

fn generate_water_areas_internal(
    editor: &mut WorldEditor,
    element: &ProcessedRelation,
    fill_outside: bool,
) {
    let start_time = Instant::now();

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

    for (i, outer_nodes) in outers.iter().enumerate() {
        let mut individual_outers = vec![outer_nodes.clone()];

        merge_loopy_loops(&mut individual_outers);
        if !verify_loopy_loops(&individual_outers) {
            println!(
                "Skipping invalid outer polygon {} for relation {}",
                i + 1,
                element.id
            );
            continue;
        }

        merge_loopy_loops(&mut inners);
        if !verify_loopy_loops(&inners) {
            let empty_inners: Vec<Vec<ProcessedNode>> = vec![];
            let mut temp_inners = empty_inners;
            merge_loopy_loops(&mut temp_inners);

            let (min_x, min_z) = editor.get_min_coords();
            let (max_x, max_z) = editor.get_max_coords();
            let individual_outers_xz: Vec<Vec<XZPoint>> = individual_outers
                .iter()
                .map(|x| x.iter().map(|y| y.xz()).collect::<Vec<_>>())
                .collect();
            let empty_inners_xz: Vec<Vec<XZPoint>> = vec![];

            let default_level = editor.ground_level();
            let water_level = if let Some(ground) = editor.get_ground() {
                let outer_points = individual_outers_xz
                    .iter()
                    .flatten()
                    .map(|pt| XZPoint::new(pt.x - min_x, pt.z - min_z));
                ground.min_level(outer_points).unwrap_or(default_level)
            } else {
                default_level
            };

            inverse_floodfill(
                min_x,
                min_z,
                max_x,
                max_z,
                individual_outers_xz,
                empty_inners_xz,
                water_level,
                editor,
                start_time,
                fill_outside,
            );
            continue;
        }

        let (min_x, min_z) = editor.get_min_coords();
        let (max_x, max_z) = editor.get_max_coords();
        let individual_outers_xz: Vec<Vec<XZPoint>> = individual_outers
            .iter()
            .map(|x| x.iter().map(|y| y.xz()).collect::<Vec<_>>())
            .collect();
        let inners_xz: Vec<Vec<XZPoint>> = inners
            .iter()
            .map(|x| x.iter().map(|y| y.xz()).collect::<Vec<_>>())
            .collect();

        let default_level = editor.ground_level();
        let water_level = if let Some(ground) = editor.get_ground() {
            let outer_points = individual_outers_xz
                .iter()
                .flatten()
                .map(|pt| XZPoint::new(pt.x - min_x, pt.z - min_z));
            ground.min_level(outer_points).unwrap_or(default_level)
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
        return;
    }

    let outer_xz: Vec<XZPoint> = way.nodes.iter().map(|n| n.xz()).collect();
    let (min_x, min_z) = editor.get_min_coords();
    let (max_x, max_z) = editor.get_max_coords();

    let default_level = editor.ground_level();
    let water_level = if let Some(ground) = editor.get_ground() {
        let outer_points = outer_xz
            .iter()
            .map(|pt| XZPoint::new(pt.x - min_x, pt.z - min_z));
        ground.min_level(outer_points).unwrap_or(default_level)
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
    );
}

pub fn generate_water_area_from_way(editor: &mut WorldEditor, way: &ProcessedWay) {
    generate_water_area_from_way_internal(editor, way, false);
}

pub fn generate_coastlines(editor: &mut WorldEditor, ways: &[Vec<ProcessedNode>]) {
    if ways.is_empty() {
        return;
    }

    let (min_x, min_z) = editor.get_min_coords();
    let (max_x, max_z) = editor.get_max_coords();
    let width = (max_x - min_x + 1) as usize;
    let height = (max_z - min_z + 1) as usize;

    let mut barrier = vec![vec![false; width]; height];

    for way in ways {
        for pair in way.windows(2) {
            let a = &pair[0];
            let b = &pair[1];
            for (x, _, z) in bresenham_line(a.x, 0, a.z, b.x, 0, b.z) {
                if x < min_x || x > max_x || z < min_z || z > max_z {
                    continue;
                }
                let gx = (x - min_x) as usize;
                let gz = (z - min_z) as usize;
                barrier[gz][gx] = true;
            }
        }
    }

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
    let water_level = editor.ground_level();

    for z in 0..height {
        for x in 0..width {
            if outside[z][x] || barrier[z][x] {
                let world_x = min_x + x as i32;
                let world_z = min_z + z as i32;
                if let Some(ref g) = ground {
                    let terrain = g.level(XZPoint::new(world_x - min_x, world_z - min_z));
                    if terrain >= water_level {
                        for y in water_level..=terrain {
                            editor.set_block_absolute(AIR, world_x, y, world_z, None, Some(&[]));
                        }
                    }
                }
                editor.set_block_absolute(WATER, world_x, water_level, world_z, None, Some(&[]));
            }
        }
    }
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
) {
    // Check if we've exceeded 25 seconds
    if start_time.elapsed().as_secs() > 25 {
        // Fall back: brute-force fill for the remaining region so we never leave it empty.
        inverse_floodfill_iterative(min, max, water_level, outers, inners, editor, fill_outside);
        return;
    }

    const ITERATIVE_THRES: i64 = 10_000;

    if min.0 > max.0 || min.1 > max.1 {
        return;
    }

    // Multiply as i64 to avoid overflow; in release builds where unchecked math is
    // enabled, this could cause the rest of this code to end up in an infinite loop.
    if ((max.0 - min.0) as i64) * ((max.1 - min.1) as i64) < ITERATIVE_THRES {
        inverse_floodfill_iterative(min, max, water_level, outers, inners, editor, fill_outside);
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
            rect_fill(min_x, max_x, min_z, max_z, water_level, editor);
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
                        for y in water_level..=terrain {
                            editor.set_block_absolute(AIR, x, y, z, None, Some(&[]));
                        }
                    }
                }
                editor.set_block_absolute(WATER, x, water_level, z, None, Some(&[]));
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
) {
    let ground = editor.get_ground().cloned();
    let (min_x_world, min_z_world) = editor.get_min_coords();
    for x in min_x..max_x {
        for z in min_z..max_z {
            if let Some(ref g) = ground {
                let terrain = g.level(XZPoint::new(x - min_x_world, z - min_z_world));
                if terrain >= water_level {
                    for y in water_level..=terrain {
                        editor.set_block_absolute(AIR, x, y, z, None, Some(&[]));
                    }
                }
            }
            editor.set_block_absolute(WATER, x, water_level, z, None, Some(&[]));
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
                    // Higher terrain should be excavated
                    assert_eq!(editor.get_block_absolute(x, 4, z), None, "x {x} z {z}");
                    assert_eq!(editor.get_block_absolute(x, 5, z), None, "x {x} z {z}");
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
