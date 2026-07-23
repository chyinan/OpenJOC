use openjoc_bitio::BitReader;
use openjoc_joc::{HuffmanError, HuffmanTable, all_huffman_tables, decode_huffman};
use std::collections::HashSet;

fn leaf_paths(table: HuffmanTable) -> Vec<(Vec<bool>, u16)> {
    fn visit(
        nodes: &[[i16; 2]],
        node: usize,
        path: &mut Vec<bool>,
        leaves: &mut Vec<(Vec<bool>, u16)>,
        active: &mut HashSet<usize>,
    ) {
        assert!(active.insert(node), "cycle at node {node}");
        for branch in 0..2 {
            path.push(branch != 0);
            let child = nodes[node][branch];
            if child > 0 {
                let child = usize::try_from(child).expect("positive node index");
                assert!(child < nodes.len(), "node reference out of range");
                visit(nodes, child, path, leaves, active);
            } else {
                let symbol = u16::try_from(-i32::from(child) - 1).expect("valid leaf symbol");
                leaves.push((path.clone(), symbol));
            }
            path.pop();
        }
        active.remove(&node);
    }

    let mut leaves = Vec::new();
    visit(
        table.nodes,
        0,
        &mut Vec::new(),
        &mut leaves,
        &mut HashSet::new(),
    );
    leaves
}

fn pack_bits(bits: &[bool]) -> Vec<u8> {
    let mut bytes = vec![0_u8; bits.len().div_ceil(8)];
    for (index, bit) in bits.iter().copied().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

#[test]
fn every_normative_leaf_decodes_to_its_clause_6_6_3_symbol() {
    for table in all_huffman_tables() {
        for (path, expected) in leaf_paths(table) {
            let bytes = pack_bits(&path);
            let mut reader = BitReader::new(&bytes);
            assert_eq!(
                decode_huffman(&mut reader, table.nodes, Some(path.len())),
                Ok(expected),
                "table {}, path {path:?}",
                table.name
            );
        }
    }
}

#[test]
fn every_tree_has_unique_prefix_free_leaf_paths() {
    for table in all_huffman_tables() {
        let paths = leaf_paths(table);
        let unique = paths.iter().map(|(path, _)| path).collect::<HashSet<_>>();
        assert_eq!(unique.len(), paths.len(), "duplicate in {}", table.name);
        for (left, _) in &paths {
            for (right, _) in &paths {
                if left != right {
                    assert!(
                        !right.starts_with(left),
                        "{} is not prefix-free",
                        table.name
                    );
                }
            }
        }
    }
}

#[test]
fn every_nonempty_truncated_leaf_path_returns_error() {
    for table in all_huffman_tables() {
        for (mut path, _) in leaf_paths(table) {
            path.pop();
            let bytes = pack_bits(&path);
            let mut reader = BitReader::new(&bytes);
            assert!(matches!(
                decode_huffman(&mut reader, table.nodes, Some(path.len())),
                Err(HuffmanError::TruncatedCodeword)
            ));
        }
    }
}

#[test]
fn malformed_tree_reference_is_rejected() {
    let mut reader = BitReader::new(&[0]);
    assert!(matches!(
        decode_huffman(&mut reader, &[[7, -1]], Some(1)),
        Err(HuffmanError::InvalidNode {
            node: 7,
            node_count: 1
        })
    ));
}
