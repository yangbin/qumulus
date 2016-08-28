//! Contains functions to help measure size / population statistics of Nodes and help decide the
//! appropriate points in the tree to partition as Zones.

use std::collections::BinaryHeap;

use time;

use node::Node;
use path::Path;

/// Possibly delegate
pub fn delegate(node: &Node) -> Option<Node> {
    // TODO: allow other strategies

    let (_, delegate_node) = check_node(node);
    delegate_node
}

fn check_node(node: &Node) -> (usize, Option<Node>) {
    let mut delegate_node: Node = Default::default();
    let mut total_size = node.byte_size();

    if total_size > 32768 {
        // TODO: delegate this Node if value stored here is e.g. > 32k
    }
    // TODO: handle if Node has many children, e.g. > 10000
    else {
        // recursively check if children need to be delegated

        let mut largest_children = BinaryHeap::new();

        node.each_child(|k, child_node| {
            let (mut child_size, child_delegations) = check_node(child_node);

            if let Some(child_delegations) = child_delegations {
                delegate_node.add_child(k.clone(), child_delegations);
            }

            child_size += k.len();
            total_size += child_size;

            if child_size > 1024 {
                largest_children.push( (child_size, k.clone()) );
            }
        });

        while total_size > 65535 {
            if let Some( (child_size, k) ) = largest_children.pop() {
                delegate_node.add_child(k.clone(), Node::delegate(time::precise_time_ns()));
                total_size -= child_size;
            }
            else {
                break;
            }
        }
    }

    let delegate_node = if delegate_node.is_noop() { None } else { Some(delegate_node) };

    (total_size, delegate_node)
}
