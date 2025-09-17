use std::collections::HashMap;

use crate::block_definitions::{BlockWithProperties, CHAIN, OAK_FENCE, STONE_BRICKS};
use crate::bresenham::bresenham_line;
use crate::osm_parser::{ProcessedNode, ProcessedRelation, ProcessedWay};
use crate::world_editor::WorldEditor;
use fastnbt::Value;

const MAIN_LINE_HEIGHT: i32 = 10;
const MINOR_LINE_HEIGHT: i32 = 6;

fn height_for_power_line(power_value: &str) -> Option<i32> {
    match power_value {
        "line" => Some(MAIN_LINE_HEIGHT),
        "minor_line" => Some(MINOR_LINE_HEIGHT),
        _ => None,
    }
}

fn height_for_power_node(power_value: &str) -> Option<i32> {
    match power_value {
        "tower" => Some(MAIN_LINE_HEIGHT),
        "pole" => Some(MINOR_LINE_HEIGHT),
        _ => None,
    }
}

fn build_power_pole(
    editor: &mut WorldEditor,
    node: &ProcessedNode,
    pole_height: i32,
) -> Option<(i32, i32, i32)> {
    if pole_height <= 0 {
        return None;
    }

    let base_ground_y = editor.get_absolute_y(node.x, 0, node.z);
    let stone_y = base_ground_y + 1;
    editor.set_block_absolute(STONE_BRICKS, node.x, stone_y, node.z, None, None);

    if pole_height <= 1 {
        return Some((node.x, stone_y, node.z));
    }

    let top_y = editor.get_absolute_y(node.x, pole_height, node.z);

    for y in (stone_y + 1)..=top_y {
        editor.set_block_absolute(OAK_FENCE, node.x, y, node.z, None, None);
    }

    Some((node.x, top_y, node.z))
}

fn axis_from_delta(a: (i32, i32, i32), b: (i32, i32, i32)) -> Option<&'static str> {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let dz = a.2 - b.2;

    if dx != 0 {
        Some("x")
    } else if dz != 0 {
        Some("z")
    } else if dy != 0 {
        Some("y")
    } else {
        None
    }
}

fn determine_chain_axis(
    previous: Option<(i32, i32, i32)>,
    current: (i32, i32, i32),
    next: Option<(i32, i32, i32)>,
) -> &'static str {
    if let Some(prev) = previous {
        if let Some(axis) = axis_from_delta(current, prev) {
            return axis;
        }
    }

    if let Some(next_point) = next {
        if let Some(axis) = axis_from_delta(next_point, current) {
            return axis;
        }
    }

    "y"
}

fn span_power_wires(editor: &mut WorldEditor, pole_tops: &[(i32, i32, i32)]) {
    if pole_tops.len() < 2 {
        return;
    }

    for window in pole_tops.windows(2) {
        let start = window[0];
        let end = window[1];

        if start == end {
            continue;
        }

        let line = bresenham_line(start.0, start.1, start.2, end.0, end.1, end.2);

        if line.len() <= 2 {
            continue;
        }

        for index in 1..(line.len() - 1) {
            let previous = line.get(index - 1).copied();
            let current = line[index];
            let next = line.get(index + 1).copied();
            let axis = determine_chain_axis(previous, current, next);

            let mut props = HashMap::with_capacity(1);
            props.insert("axis".to_string(), Value::String(axis.to_string()));

            editor.set_block_with_properties_absolute(
                BlockWithProperties::new(CHAIN, Some(Value::Compound(props))),
                current.0,
                current.1,
                current.2,
                None,
                None,
            );
        }
    }
}

pub fn generate_power_lines(editor: &mut WorldEditor, way: &ProcessedWay) {
    let Some(height) = way
        .tags
        .get("power")
        .and_then(|power_value| height_for_power_line(power_value.as_str()))
    else {
        return;
    };

    let mut pole_tops: Vec<(i32, i32, i32)> = Vec::with_capacity(way.nodes.len());

    for node in &way.nodes {
        if let Some(top) = build_power_pole(editor, node, height) {
            pole_tops.push(top);
        }
    }

    span_power_wires(editor, &pole_tops);
}

pub fn generate_power_node(editor: &mut WorldEditor, node: &ProcessedNode) {
    let Some(height) = node
        .tags
        .get("power")
        .and_then(|power_value| height_for_power_node(power_value.as_str()))
    else {
        return;
    };

    build_power_pole(editor, node, height);
}

pub fn generate_power_relation(_editor: &mut WorldEditor, _relation: &ProcessedRelation) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_definitions::{CHAIN, OAK_FENCE, STONE_BRICKS};
    use crate::bresenham::bresenham_line;
    use crate::coordinate_system::cartesian::XZBBox;
    use crate::coordinate_system::geographic::LLBBox;
    use crate::ground::Ground;
    use crate::osm_parser::{ProcessedNode, ProcessedWay};
    use fastanvil::Region;
    use fastnbt::Value;
    use std::collections::HashMap;
    use std::fs::File;
    use tempfile::tempdir;

    fn palette_index_at(
        block_states: &HashMap<String, Value>,
        local_x: i32,
        local_y: i32,
        local_z: i32,
    ) -> Option<usize> {
        let index = (local_y as usize) * 256 + (local_z as usize) * 16 + (local_x as usize);
        match block_states.get("data") {
            Some(Value::LongArray(data)) => {
                if data.is_empty() {
                    return Some(0);
                }

                let data_vec: Vec<i64> = data.iter().copied().collect();
                if data_vec.is_empty() {
                    return Some(0);
                }

                let bits_per_block = (data_vec.len() * 64) / 4096;
                if bits_per_block == 0 {
                    return Some(0);
                }

                let mask = (1u64 << bits_per_block) - 1;
                let mut indices = Vec::with_capacity(4096);
                let mut iter = data_vec.iter();
                let mut current = *iter.next().unwrap() as u64;
                let mut current_bit_index = 0;

                for _ in 0..4096 {
                    if current_bit_index + bits_per_block > 64 {
                        current = *iter.next().unwrap_or(&0) as u64;
                        current_bit_index = 0;
                    }
                    let palette_idx = ((current >> current_bit_index) & mask) as usize;
                    current_bit_index += bits_per_block;
                    indices.push(palette_idx);
                }

                indices.get(index).copied()
            }
            _ => Some(0),
        }
    }

    fn extract_axis_property(chunk: &Value, x: i32, y: i32, z: i32) -> Option<String> {
        let sections = match chunk {
            Value::Compound(map) => map.get("sections")?,
            _ => return None,
        };

        let section_y = y >> 4;

        if let Value::List(section_list) = sections {
            for section in section_list {
                let Value::Compound(section_map) = section else {
                    continue;
                };

                match section_map.get("Y") {
                    Some(Value::Byte(value)) if *value as i32 == section_y => {}
                    _ => continue,
                }

                let block_states = match section_map.get("block_states") {
                    Some(Value::Compound(map)) => map,
                    _ => continue,
                };

                let palette = match block_states.get("palette") {
                    Some(Value::List(list)) => list,
                    _ => continue,
                };

                let local_x = x & 15;
                let local_y = y & 15;
                let local_z = z & 15;

                let palette_index = palette_index_at(block_states, local_x, local_y, local_z)?;
                let palette_entry = match palette.get(palette_index) {
                    Some(Value::Compound(entry)) => entry,
                    _ => continue,
                };

                if let Some(Value::Compound(properties)) = palette_entry.get("Properties") {
                    if let Some(Value::String(axis)) = properties.get("axis") {
                        return Some(axis.clone());
                    }
                }
            }
        }

        None
    }

    #[test]
    fn power_line_places_poles_and_oriented_wires() {
        let dir = tempdir().unwrap();
        std::fs::create_dir(dir.path().join("region")).unwrap();

        let xzbbox = XZBBox::rect_from_xz_lengths(32.0, 32.0).unwrap();
        let llbbox = LLBBox::new(0.0, 0.0, 1.0, 1.0).unwrap();
        let mut editor = WorldEditor::new(dir.path().to_path_buf(), &xzbbox, llbbox);
        let ground = Ground::new_flat(0);
        editor.set_ground(&ground);

        let node_a = ProcessedNode {
            id: 1,
            tags: HashMap::new(),
            x: 0,
            z: 0,
        };
        let node_b = ProcessedNode {
            id: 2,
            tags: HashMap::new(),
            x: 5,
            z: 0,
        };

        let mut tags = HashMap::new();
        tags.insert("power".to_string(), "line".to_string());

        let way = ProcessedWay {
            id: 99,
            nodes: vec![node_a.clone(), node_b.clone()],
            tags,
        };

        let top_a = editor.get_absolute_y(node_a.x, MAIN_LINE_HEIGHT, node_a.z);
        let top_b = editor.get_absolute_y(node_b.x, MAIN_LINE_HEIGHT, node_b.z);
        let base_a = editor.get_absolute_y(node_a.x, 0, node_a.z);

        generate_power_lines(&mut editor, &way);

        assert_eq!(
            editor.get_block_absolute(node_a.x, base_a + 1, node_a.z),
            Some(STONE_BRICKS)
        );
        assert_eq!(
            editor.get_block_absolute(node_b.x, base_a + 1, node_b.z),
            Some(STONE_BRICKS)
        );

        for y in (base_a + 2)..=top_a {
            assert_eq!(
                editor.get_block_absolute(node_a.x, y, node_a.z),
                Some(OAK_FENCE)
            );
        }
        for y in (base_a + 2)..=top_b {
            assert_eq!(
                editor.get_block_absolute(node_b.x, y, node_b.z),
                Some(OAK_FENCE)
            );
        }

        let line_points = bresenham_line(node_a.x, top_a, node_a.z, node_b.x, top_b, node_b.z);
        assert!(line_points.len() > 2);
        let first_wire = line_points[1];
        assert_eq!(
            editor.get_block_absolute(first_wire.0, first_wire.1, first_wire.2),
            Some(CHAIN)
        );

        editor.save();

        let region_path = dir.path().join("region").join("r.0.0.mca");
        let mut region = Region::from_stream(File::open(region_path).unwrap()).unwrap();
        let chunk_bytes = region.read_chunk(0, 0).unwrap().unwrap();
        let chunk_value: Value = fastnbt::from_bytes(&chunk_bytes).unwrap();

        let axis = extract_axis_property(&chunk_value, first_wire.0, first_wire.1, first_wire.2)
            .expect("axis property");
        assert_eq!(axis, "x");
    }
}
