use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use serde_json::Value;

use path::Path;

/// Tracks visibility of a node
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vis {
    updated: u64,
    deleted: u64
}

#[derive(Debug, PartialEq)]
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
    keys: Option<BTreeMap<String, Update>>
    // TODO: external status
}

pub struct External {
    path: Path,
    p_min_updated: u64,
    p_max_deleted: u64,
    node: Node
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
    pub fn new(updated: u64, deleted: u64) -> Vis {
        Vis { updated: updated, deleted: deleted }
    }

    fn is_visible(&self) -> bool {
        self.updated > self.deleted
    }

    fn merge(&mut self, diff: &Vis) {
        if diff.updated > self.updated {
            self.updated = diff.updated;
        }

        if diff.deleted > self.deleted {
            self.deleted = diff.deleted;
        }
    }

    /// Returns new effective visibility given child visibility
    fn descend(&mut self, child: &Vis) {
        if child.updated < self.updated { self.updated = child.updated }
        if child.deleted > self.deleted { self.deleted = child.deleted }
    }

    fn is_noop(&self) -> bool {
        self.updated == 0 && self.deleted == 0
    }
}

impl Default for Node {
    fn default() -> Node {
        Node {
            vis: Vis { updated: 0, deleted: 0 },
            value: Value::Null,
            keys: None,
            delegated: 0
        }
    }
}

impl Node {
    pub fn delete(timestamp: u64) -> Node {
        Node {
            vis: Vis {
                deleted: timestamp,
                ..Default::default()
             },
             ..Default::default()
        }
    }

    pub fn expand(data: &Value, timestamp: u64) -> Node {
        if let Value::Array(ref arr) = *data {
            let keys = arr.iter().enumerate().map(|(k, v)|
                (k.to_string(), Node::expand(&v, timestamp))
            ).collect();

            Node {
                vis: Vis { updated: timestamp, ..Default::default() },
                keys: Some(keys),
                ..Default::default()
            }
        }
        else if let Value::Object(ref obj) = *data {
            let keys = obj.iter().map(|(k, v)|
                (k.clone(), Node::expand(&v, timestamp))
            ).collect();

            Node {
                vis: Vis { updated: timestamp, ..Default::default() },
                keys: Some(keys),
                ..Default::default()
            }
        }
        else {
            Node {
                vis: Vis { updated: timestamp, ..Default::default() },
                value: data.clone(),
                ..Default::default()
            }
        }
    }

    pub fn expand_from(path: &[String], data: &Value, timestamp: u64) -> Node {
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

    fn is_noop(&self) -> bool {
        self.vis.is_noop() && self.value == Value::Null && self.keys.is_none()
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
    /// * `diff` - Set of changes to be applied. Modified to retain only actual changes.
    /// * `vis_old` - The previous `Vis` timestamps of ancestor nodes.
    /// * `vis_new` - The next `Vis` timestamps of ancestor nodes.
    ///
    /// # Return value
    ///
    /// The return value is a tuple of:
    ///
    /// * `updates` - a nested map of `Update`s to be sent to listeners, and
    /// * `externals` - a Vec of External changes to be applied to other zones.
    pub fn merge(&mut self,
                 diff: &mut Node,
                 vis_old: Vis,
                 vis_new: Vis
                ) -> (Option<Update>, Vec<External>) {
        let mut externals: Vec<External> = vec![];

        let mut stack = Path::new(vec![]);

        let update = merge(&mut stack, self, diff, vis_old, vis_new, &mut externals);

        (update, externals)
    }

    /// Read data from node
    ///
    /// Returns user-visible data at `path`.
    pub fn read(&self, vis: Vis, path: &Path) -> (Option<Update>, Vec<External>) {
        let mut externals: Vec<External> = vec![];

        let mut stack = Path::new(vec![]);

        let update = read(&mut stack, self, vis, path, 0, &mut externals);

        (update, externals)
    }
}

impl Update {
    pub fn to_json(&self) -> Value {
        let value = self.new.clone().unwrap_or(Value::Null);

        let keys = match self.keys {
            None => Value::Null,
            Some(ref keys) => Value::Object(keys.iter().map(|(k, v)|
                (k.clone(), v.to_json())
            ).collect())
        };

        Value::Array(vec![value, keys])
    }
}

/// Internal merge implementation function. Function is recursive and tracks `path`.
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

    // Merge value at node

    if diff.vis.updated > node.vis.updated {
        // timestamp newer, use updated value
        node.value = diff.value.clone();
        node.vis.updated = diff.vis.updated;
        propagate = Some(Default::default());
    }
    else if diff.vis.updated < node.vis.updated {
        // outdated diff, throw away
        diff.vis.updated = 0;
        diff.value = Value::Null;
    }
    else { // same timesstamp
        if diff.value == node.value {
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

    if vis_old.is_visible() != vis_new.is_visible() {
        update.visible = Some(vis_new.is_visible());
        update.new = Some(node.value.clone());
    }
    else {
        update.old = None;
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

    if node.delegated & 1 > 0 {
        unimplemented!(); // TODO throw everything we have to external node
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
        externals: &mut Vec<External>)
-> Option<Update> {
    // Effective visibility of this node
    vis.descend(&node.vis);

    let mut update: Update = Default::default();

    let pos = stack.len();

    if pos >= path.len() {
        // Get value at this node
        if vis.is_visible() {
            update.visible = Some(vis.is_visible());
            update.new = Some(node.value.clone());
        }
    }
    else {
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

    // Delegated data
    if node.delegated & 1 > 0 {
        unimplemented!(); // TODO throw everything we have to external node
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
    let data: Value = serde_json::from_str(r#"
        {
            "moo": 42
        }
    "#).unwrap();

    let node = Node::expand(&data, 1000);

    let expected = Node {
        vis: Vis {
            updated: 1000,
            deleted: 0
        },
        value:  Value::Null,
        keys: Some(map! {
            "moo".to_string() => Node {
                vis: Vis {
                    updated: 1000,
                    deleted: 0
                },
                value: serde_json::to_value(&42),
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
