use petgraph::stable_graph::NodeIndex;
use petgraph::stable_graph::StableDiGraph;
use petgraph::visit::EdgeRef;

use std::{
    collections::{HashMap},
    path::{Path, PathBuf},
    str::FromStr,
};

use super::IncludePosition;

/// Wraps a `StableDiGraph` with caching behaviour for node search by maintaining
/// an index for node value to node index and a reverse index.
/// This allows for **O(1)** lookup for a value if it exists, else **O(n)**.
pub struct CachedStableGraph {
    // StableDiGraph is used as it allows for String node values, essential for
    // generating the GraphViz DOT render.
    pub graph: StableDiGraph<String, IncludePosition>,
    cache: HashMap<PathBuf, NodeIndex>,
}

impl CachedStableGraph {
    #[allow(clippy::new_without_default)]
    pub fn new() -> CachedStableGraph {
        CachedStableGraph {
            graph: StableDiGraph::new(),
            cache: HashMap::new(),
        }
    }

    /// Returns the `NodeIndex` for a given graph node with the value of `name`
    /// and caches the result in the `HashMap`. Complexity is **O(1)** if the value
    /// is cached (which should always be the case), else **O(n)** where **n** is
    /// the number of node indices, as an exhaustive search must be done.
    pub fn find_node(&mut self, name: &Path) -> Option<NodeIndex> {
        match self.cache.get(name) {
            Some(n) => Some(*n),
            None => {
                // If the string is not in cache, O(n) search the graph (i know...) and then cache the NodeIndex
                // for later
                let n = self.graph.node_indices().find(|n| self.graph[*n] == name.to_str().unwrap());
                if let Some(n) = n {
                    self.cache.insert(name.into(), n);
                }
                n
            }
        }
    }

    // Returns the `PathBuf` for a given `NodeIndex`
    pub fn get_node(&self, node: NodeIndex) -> PathBuf {
        PathBuf::from_str(&self.graph[node]).unwrap()
    }

    /// Returns an iterator over all the `IncludePosition`'s between a parent and its child for all the positions
    /// that the child may be imported into the parent, in order of import.
    pub fn get_child_positions(&self, parent: NodeIndex, child: NodeIndex) -> impl Iterator<Item = IncludePosition> + '_ {
        let mut edges = self
            .graph
            .edges(parent)
            .filter_map(move |edge| {
                let target = self.graph.edge_endpoints(edge.id()).unwrap().1;
                if target != child {
                    return None;
                }
                Some(self.graph[edge.id()])
            })
            .collect::<Vec<IncludePosition>>();
        edges.sort_by(|x, y| x.line.cmp(&y.line));
        edges.into_iter()
    }

    pub fn child_node_indexes(&self, node: NodeIndex) -> impl Iterator<Item = NodeIndex> + '_ {
        self.graph.neighbors(node)
    }
}
