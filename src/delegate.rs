//! Contains functions to help measure size / population statistics of Nodes and help decide the
//! appropriate points in the tree to partition as Zones.

use time;

use node::Node;
use path::Path;

/// Possibly delegate
pub fn delegate(node: &Node) -> Option<Node> {
    // TODO: allow other strategies

    let path = delegate_largest_innermost_parent(node);

    match path {
        Some(path) => {
            if path.len() > 0 {
                Some(Node::delegate(time::precise_time_ns()).prepend_path(&path.path))
            }
            else {
                None
            }
        },
        None => None
    }
}

/// This algorithm delegates the parent whose total size is closest to half the top-level size
pub fn delegate_largest_innermost_parent(node: &Node) -> Option<Path> {
    let (size, mut max_path) = node.max_bytes_path();

    let mut path = vec![];

    if size > 64000 { // TODO: make configurable
        let target_size = size as i64 >> 1;
        let mut min_diff = size as i64;

        for (key, size) in max_path.drain(..) {
            let this_diff = (size as i64 - target_size).abs();

            if this_diff < min_diff {
                min_diff = this_diff;
                path.clear();
            }

            path.push(key);
        }

        path.reverse();

        Some(Path::new(path))
    }
    else {
        None
    }
}
