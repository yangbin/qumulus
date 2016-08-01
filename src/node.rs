//! Qumulus's CRDT representation.
//!
//! Data is represented as a hierarchical / nested maps-of-maps, with the ability to associate
//! values with any path, and to use any path as a map.
//!
//! For each 'node' in the tree, two timestamps are tracked as meta information. These timestamps
//! are used to for consistent conflict resolution.
//!
//! Deleted data leave meta information as tombstones which are occasionally cleared [TODO].

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::mem;

use serde_json::Value as JSON;

use path::Path;
use value::Value;

/// Tracks visibility of a node
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Vis {
    updated: u64,
    deleted: u64
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Node {
    vis: Vis,
    value: Value,
    keys: Option<BTreeMap<String, Node>>,
    delegated: u64
}

#[derive(Debug, Default, PartialEq)]
pub struct Update {
    visible: Option<bool>,
    old: Option<Value>,
    new: Option<Value>,
    keys: Option<BTreeMap<String, Update>>,
    delegated: Option<bool>
}

#[derive(Debug, Default)]
pub struct External {
    /// Path to delegated data
    pub path: Path,

    /// Effective visibility of parent node
    pub parent_vis: Vis,

    /// Data to be delegated (relative)
    pub node: Node
}

#[derive(Debug, Default)]
pub struct DelegatedMatch {
    /// Path to delegated data
    pub path: Path,

    /// Relative path / match spec
    pub match_spec: Path
}

macro_rules! map(
    { $($key:expr => $value:expr),+ } => {
        {
            let mut m = BTreeMap::new();
            $(
                m.insert($key, $value);
            )+
            m
        }
    };
);

impl Vis {
    /// Creates a new `Vis` with given `updated` and `deleted` timestamps.
    pub fn new(updated: u64, deleted: u64) -> Vis {
        Vis { updated: updated, deleted: deleted }
    }

    /// Creates a new `Vis` with given `updated` timestamp.
    pub fn update(updated: u64) -> Vis { Vis::new(updated, 0) }

    /// Creates a new `Vis` with given `deleted` timestamp.
    pub fn delete(deleted: u64) -> Vis { Vis::new(0, deleted) }

    /// Returns a `Vis` that's always visible.
    pub fn permanent() -> Vis { Vis::update(u64::max_value()) }

    /// Returns new effective visibility given child visibility.
    pub fn descend(&mut self, child: &Vis) {
        if child.updated < self.updated { self.updated = child.updated }
        if child.deleted > self.deleted { self.deleted = child.deleted }
    }

    /// Returns true if merging this `Vis` does nothing. Also `Default::default()`.
    pub fn is_noop(&self) -> bool {
        self.updated == 0 && self.deleted == 0
    }

    /// Returns visibility of `Vis`.
    pub fn is_visible(&self) -> bool {
        self.updated > self.deleted
    }

    /// Resolve `Vis` conflicts by keeping newest data.
    pub fn merge(&mut self, diff: &Vis) {
        if diff.updated > self.updated {
            self.updated = diff.updated;
        }

        if diff.deleted > self.deleted {
            self.deleted = diff.deleted;
        }
    }
}

impl Default for Node {
    fn default() -> Node {
        Node {
            vis: Default::default(),
            value: Value::Null,
            keys: None,
            delegated: 0
        }
    }
}

impl Node {
    /// Creates a `Node` representing a recursive delete with given `timestamp`.
    pub fn delete(timestamp: u64) -> Node {
        Node {
            vis: Vis::delete(timestamp),
             ..Default::default()
        }
    }

    /// Expands JSON data to a `Node` representation creating each node at given `timestamp`.
    pub fn expand(data: JSON, timestamp: u64) -> Node {
        let vis = Vis::update(timestamp);

        match data {
            JSON::Null => Node { vis: vis, value: Value::Null, ..Default::default() },
            JSON::Bool(v) => Node { vis: vis, value: Value::Bool(v), ..Default::default() },
            JSON::I64(v) => Node { vis: vis, value: Value::I64(v), ..Default::default() },
            JSON::U64(v) => Node { vis: vis, value: Value::U64(v), ..Default::default() },
            JSON::F64(v) => Node { vis: vis, value: Value::F64(v), ..Default::default() },
            JSON::String(s) => Node { vis: vis, value: Value::from(s), ..Default::default() },
            JSON::Object(obj) => {
                let keys = obj.into_iter().map(|(k, v)|
                    (k, Node::expand(v, timestamp))
                ).collect();

                Node {
                    vis: vis,
                    keys: Some(keys),
                ..Default::default()
                }
            },
            JSON::Array(arr) => {
                let keys = arr.into_iter().enumerate().map(|(k, v)|
                    (k.to_string(), Node::expand(v, timestamp))
                ).collect();

                Node {
                    vis: vis,
                    keys: Some(keys),
                ..Default::default()
                }
            }
        }
    }

    pub fn expand_from(path: &[String], data: JSON, timestamp: u64) -> Node {
        // TODO: make iterative
        match path.len() {
            0 => Node::expand(data, timestamp),
            _ => {
                match path.split_first() {
                    Some((first, rest)) => Node {
                        keys: Some(map! {
                            first.clone() => Node::expand_from(rest, data, timestamp)
                        }),
                        ..Default::default()
                    },
                    None => Default::default()
                }
            }
        }
    }

    pub fn delegate(timestamp: u64) -> Node {
        Node {
            vis: Default::default(),
             delegated: timestamp | 1,
             ..Default::default()
        }
    }

    pub fn undelegate(timestamp: u64) -> Node {
        Node {
            vis: Default::default(),
             delegated: timestamp & !1,
             ..Default::default()
        }
    }

    /// Returns all data that should be external
    pub fn delegated(&mut self) -> Node {
        Node {
            vis: self.vis,
            value: mem::replace(&mut self.value, Value::Null),
            keys: mem::replace(&mut self.keys, None),
            delegated: self.delegated
        }
    }

    pub fn prepend_path(self, path: &[String]) -> Node {
        let mut node = self;

        for p in path.iter().rev() {
            node = Node {
                keys: Some(map! {
                    p.clone() => node
                }),
                ..Default::default()
            }
        }

        node
    }

    pub fn is_noop(&self) -> bool {
        self.vis.is_noop() && self.value == Value::Null && self.keys.is_none()
    }

    /// Returns the estimated byte size of storing this node
    pub fn byte_size(&self) -> usize {
        match self.value {
            Value::Bool(_) => 1,
            Value::I64(_) | Value::U64(_) | Value::F64(_) => 8,
            Value::String(ref s) => s.len(),
            Value::Null => 1
        }
    }

    /// Returns estimated byte size and the path from a leaf node to the root where each step of the
    /// path is the largest node and its size.
    ///
    /// TODO: This doesn't feel like it should be here
    pub fn max_bytes_path(&self) -> (usize, Vec<(String, usize)>) {
        let mut byte_size = self.byte_size();

        if let Some(ref node_keys) = self.keys {
            let mut largest_k = "";
            let mut largest_size = 0;
            let mut max_path = vec![];

            for (k, node_child) in node_keys.iter() {
                let (c_byte_size, c_max_path) = node_child.max_bytes_path();

                byte_size += k.len() + c_byte_size;

                if c_byte_size > largest_size {
                    largest_size = c_byte_size;
                    largest_k = k;
                    max_path = c_max_path;
                }
            }

            if largest_size > 0 {
                max_path.push((largest_k.to_string(), largest_size));
            }

            (byte_size, max_path)
        }
        else {
            (byte_size, vec![])
        }
    }

    /// Unified merge function - merges `diff` into `self` and returns changes.
    ///
    /// Returns user-visible updates based on parent's visibility, also returns
    /// list of external zones with updated data.
    ///
    /// All operations on Zone data are transformed into the merge form, which
    /// is then handled by the merge function. This allows most logic to be
    /// consolidated into the merge function allowing for easier testing.
    ///
    /// # Arguments
    ///
    /// * [in]
    ///   * `vis_old` - The previous `Vis` timestamps of ancestor nodes.
    ///   * `vis_new` - The next `Vis` timestamps of ancestor nodes.
    /// * [in/out]
    ///   * `diff` - Set of changes to be applied. Modified to retain only actual changes.
    /// * [out]
    ///   * `updates` - a nested map of `Update`s to be sent to listeners, and
    ///   * `externals` - a Vec of External changes to be applied to other zones.
    pub fn merge(&mut self,
                 diff: &mut Node,
                 vis_old: Vis,
                 vis_new: Vis
                ) -> (Option<Update>, Vec<External>) {
        let mut externals: Vec<External> = vec![];

        let mut stack = Path::empty();

        let update = merge(&mut stack, self, diff, vis_old, vis_new, &mut externals);

        (update, externals)
    }

    /// Read data from node
    ///
    /// Returns user-visible data at `path`.
    pub fn read(&self, vis: Vis, path: &Path) -> (Option<Update>, Vec<DelegatedMatch>) {
        let mut externals = vec![];

        let mut stack = Path::empty();

        let update = read(&mut stack, self, vis, path, 0, &mut externals);

        (update, externals)
    }
}

impl Update {
    pub fn to_json(&self) -> JSON {
        let visible = match self.visible {
            None => JSON::Null,
            Some(false) => JSON::Bool(false),
            Some(true) => JSON::Bool(true)
        };

        let value = match self.new {
            Some(Value::Null) | None => JSON::Null,
            Some(Value::Bool(v)) => JSON::Bool(v),
            Some(Value::I64(v)) => JSON::I64(v),
            Some(Value::U64(v)) => JSON::U64(v),
            Some(Value::F64(v)) => JSON::F64(v),
            Some(Value::String(ref s)) => JSON::String(String::from(&**s))
        };

        let keys = match self.keys {
            None => JSON::Null,
            Some(ref keys) => JSON::Object(keys.iter().filter_map(|(k, v)|
                match v.delegated {
                    Some(true) => None,
                    _ => Some((k.clone(), v.to_json()))
                }
            ).collect())
        };

        JSON::Array(vec![keys, visible, value])
    }
}

/// Internal merge implementation function. Function is recursive, current path of `node` being
/// processed is tracked in `stack`.
fn merge(
    stack: &mut Path,
    node: &mut Node,
    diff: &mut Node,
    mut vis_old: Vis, // Old visibility of parent node
    mut vis_new: Vis, // New visibility of parent node
    externals: &mut Vec<External>)
-> Option<Update> {
    // "Previous" effective visibility of this node
    vis_old.descend(&node.vis);

    // Merge external status of node
    if diff.delegated > 0 && diff.delegated > node.delegated {
        node.delegated = diff.delegated;
    }
    else {
        diff.delegated = 0;
    }

    let mut update: Update = Default::default();

    if vis_old.is_visible() {
        update.old = Some(node.value.clone()); // TODO unnecessary copy if value / vis not changed
    }

    // If `propagate` is Some there are new timestamps for updated / deleted
    // that needs to be propagated to existing nodes. The effective visibilities
    // of this or child nodes may have changed.
    let mut propagate: Option<Node> = None;

    let mut changed = false; // set to true of value changes (ignoring vis)

    // Merge value at node

    if diff.vis.updated > node.vis.updated {
        // timestamp newer, use updated value
        if node.value != diff.value {
            node.value = diff.value.clone();
            changed = true;
        }

        node.vis.updated = diff.vis.updated;
        propagate = Some(Default::default());
    }
    else if diff.vis.updated < node.vis.updated {
        // outdated diff, throw away
        diff.vis.updated = 0;
        diff.value = Value::Null;
    }
    else { // same timesstamp
        if diff.value != node.value {
            // TODO: This isn't so good
            println!("Value conflict: {:?} - {:?} -> {:?} t+{:?}", stack, node.value, diff.value, diff.vis.updated);
        }
    }

    // Merge deletion

    if diff.vis.deleted > node.vis.deleted {
        // newer deletion, so delete
        node.vis.deleted = diff.vis.deleted;

        if node.vis.updated < node.vis.deleted {
            node.value = Value::Null;
        }

        if let Some(ref mut p_node) = propagate {
            p_node.vis.deleted = diff.vis.deleted;
        }
        else {
            propagate = Some(Node::delete(diff.vis.deleted));
        }

    }
    else {
        // outdated delete, throw away
        diff.vis.deleted = 0
    }

    // "New" effective visibility of this node
    vis_new.descend(&node.vis);

    let old_vis = vis_old.is_visible();
    let new_vis = vis_new.is_visible();

    match (old_vis, new_vis) {
        (false, false) => {
            update.old = None;
        },
        (false, true)  => {
            update.new = Some(node.value.clone());
            update.visible = Some(true);
        },
        (true, false) => {
            update.visible = Some(false);
        },
        (true, true)  => {
            if changed {
                update.new = Some(node.value.clone());
            }
            else {
                update.old = None;
            }
        }
    }

    // Propagate uncloaks / deletes
    if let Some(mut p_node) = propagate {
        if let Some(ref mut node_keys) = node.keys {
            // Uncloak / delete children
            for (k, node_child) in node_keys.iter_mut() {
                stack.push(k);

                let child_diff = merge(stack, node_child, &mut p_node, vis_old, vis_new, externals);

                stack.pop();

                update.add_child(k, child_diff);
            }
        }
    }

    // Merge keys
    if let Some(ref mut diff_keys) = diff.keys {
        if node.keys.is_none() {
            node.keys = Some(BTreeMap::new());
        }

        let node_keys = node.keys.as_mut().unwrap();

        for (k, diff_child) in diff_keys.iter_mut() {
            // TODO: unnecessary copy if key exists
            let entry = node_keys.entry(k.clone());

            stack.push(k);

            match entry {
                Entry::Occupied(mut entry) => {
                    // Existing node exists, so recursively merge
                    let child_update = merge(stack, entry.get_mut(), diff_child, vis_old, vis_new, externals);
                    update.add_child(k, child_update);
                },
                Entry::Vacant(entry) => {
                    // No existing node, merge to empty node
                    let mut node_child: Node = Default::default();

                    let child_update = merge(stack, &mut node_child, diff_child, vis_old, vis_new, externals);

                    if child_update.is_some() {
                        // If there are actual changes, keep node child
                        entry.insert(node_child);
                    }

                    update.add_child(k, child_update);
                }
            }

            stack.pop();
        }
    }

    // Handle delegation
    if stack.len() > 0 && node.delegated & 1 > 0 {
        let external = External {
            path: stack.clone(),
            parent_vis: vis_new,
            node: node.delegated()
        };

        externals.push(external);

        // TODO: at this point, delegated data has been moved, so we better not crash

        update.visible = None;
        update.old = None;
        update.new = None;
        update.keys = None;
        update.delegated = Some(true);
    }

    // TODO: throw node / diff / update away if empty

    return match update.is_noop() {
        true => None,
        false => Some(update)
    };
}

/// Internal read implementation. `stack` tracks depth of recursion.
fn read(stack: &mut Path,
        node: &Node,
        mut vis: Vis, // Visibility of parent node
        path: &Path,
        pos: usize,
        externals: &mut Vec<DelegatedMatch>)
-> Option<Update> {
    // Effective visibility of this node
    vis.descend(&node.vis);

    // Delegated data
    if stack.len() > 0 && node.delegated & 1 > 0 {
        let delegated = DelegatedMatch {
            path: stack.clone(),
            match_spec: path.slice(pos).clone()
        };

        externals.push(delegated);

        return Some(Update {
            delegated: Some(true),
            ..Default::default()
        });
    }

    let mut update: Update = Default::default();

    if stack.len() >= path.len() {
        // Get value at this node
        if vis.is_visible() {
            update.visible = Some(vis.is_visible());
            update.new = Some(node.value.clone());
        }
    }

    if pos < path.len() {
        // Match / get child values
        let ref part = path.path[pos];

        if let Some(ref node_keys) = node.keys {
            if &*part == "*" {
                // Match all
                for (k, node_child) in node_keys.iter() {
                    stack.push(k);

                    let child_update = read(stack, node_child, vis, &path, pos + 1, externals);

                    stack.pop();

                    update.add_child(k, child_update);
                }
            }
            else if &*part == "**" {
                // Match all recursively
                for (k, node_child) in node_keys.iter() {
                    stack.push(k);

                    // don't advance path position
                    let child_update = read(stack, node_child, vis, &path, pos, externals);

                    stack.pop();

                    update.add_child(k, child_update);
                }
            }
            else {
                // Match one
                match node_keys.get(part) {
                    Some(node_child) => {
                        stack.push(part);

                        let child_update = read(stack, node_child, vis, &path, pos + 1, externals);

                        stack.pop();

                        update.add_child(part, child_update);
                    },
                    None => {
                        // TODO: probably have to return an undefined
                    }
                }
            }
        }
    }

    return match update.is_noop() {
        true => None,
        false => Some(update)
    };
}

impl Update {
    fn add_child(&mut self, k: &String, child_update: Option<Update>) {
        if let Some(child_update) = child_update {
            if self.keys.is_none() {
                self.keys = Some(BTreeMap::new())
            }

            let keys = self.keys.as_mut().unwrap();

            keys.insert(k.clone(), child_update);
        }
    }

    fn is_noop(&self) -> bool {
        self.visible.is_none() &&
            self.old.is_none() &&
            self.new.is_none() &&
            self.keys.is_none()
    }
}

#[cfg(test)]
use serde_json;

#[test]
fn test_expand() {
    let data: JSON = serde_json::from_str(r#"
        {
            "moo": 42
        }
    "#).unwrap();

    let node = Node::expand(data, 1000);

    let expected = Node {
        vis: Vis::new(1000, 0),
        value:  Value::Null,
        keys: Some(map! {
            "moo".to_string() => Node {
                vis: Vis::new(1000, 0),
                value: Value::U64(42),
                keys: None,
                delegated: 0
            }
        }),
        delegated: 0
    };

    assert_eq!(node, expected);
}

#[test]
fn test_merge() {
    // TODO
}
