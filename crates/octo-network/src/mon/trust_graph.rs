//! Trust graph renderer (mission 0851p-a-trust-ux).
//!
//! Renders the web-of-trust graph (the `signed_by` relationships
//! between peers) as ASCII art or DOT (Graphviz) format for
//! operator inspection.
//!
//! ## Output formats
//!
//! - ASCII (default) — limited to ~50 nodes, suitable for
//!   terminal display.
//! - DOT — for large graphs; pipe to `dot -Tpng` to render.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A node in the trust graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustNode {
    pub peer_id: String,
    /// Optional human-readable label.
    pub label: Option<String>,
}

/// A directed edge in the trust graph: `from` trusts `to`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustEdge {
    pub from: String,
    pub to: String,
}

/// The trust graph.
#[derive(Clone, Debug, Default)]
pub struct TrustGraph {
    pub nodes: Vec<TrustNode>,
    pub edges: Vec<TrustEdge>,
}

/// Output format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphFormat {
    Ascii,
    Dot,
}

impl TrustGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node.
    pub fn add_node(&mut self, node: TrustNode) {
        if !self.nodes.iter().any(|n| n.peer_id == node.peer_id) {
            self.nodes.push(node);
        }
    }

    /// Add an edge.
    pub fn add_edge(&mut self, edge: TrustEdge) {
        self.edges.push(edge);
    }

    /// Build the trust graph from a flat list of `signed_by`
    /// relationships: `Vec<(peer_id, signed_by)>`.
    pub fn from_signed_by(relations: &[(String, String)]) -> Self {
        let mut g = Self::new();
        for (peer, signed_by) in relations {
            g.add_node(TrustNode {
                peer_id: peer.clone(),
                label: None,
            });
            g.add_node(TrustNode {
                peer_id: signed_by.clone(),
                label: None,
            });
            g.add_edge(TrustEdge {
                from: peer.clone(),
                to: signed_by.clone(),
            });
        }
        g
    }

    /// Render the graph in the requested format.
    ///
    /// For ASCII, the output is a simple node-list with their
    /// in-degree and out-degree. For DOT, the output is a valid
    /// Graphviz digraph.
    pub fn render(&self, format: GraphFormat) -> String {
        match format {
            GraphFormat::Ascii => self.render_ascii(),
            GraphFormat::Dot => self.render_dot(),
        }
    }

    fn render_ascii(&self) -> String {
        if self.nodes.is_empty() {
            return "(empty trust graph)\n".to_string();
        }
        // Compute in-degree and out-degree.
        let mut in_deg: HashMap<&str, usize> = HashMap::new();
        let mut out_deg: HashMap<&str, usize> = HashMap::new();
        for n in &self.nodes {
            in_deg.insert(&n.peer_id, 0);
            out_deg.insert(&n.peer_id, 0);
        }
        for e in &self.edges {
            *out_deg.entry(&e.from).or_insert(0) += 1;
            *in_deg.entry(&e.to).or_insert(0) += 1;
        }
        // Sort nodes by in-degree desc, then by peer_id asc.
        let mut nodes: Vec<&TrustNode> = self.nodes.iter().collect();
        nodes.sort_by(|a, b| {
            in_deg
                .get(b.peer_id.as_str())
                .unwrap_or(&0)
                .cmp(in_deg.get(a.peer_id.as_str()).unwrap_or(&0))
                .then(a.peer_id.cmp(&b.peer_id))
        });
        let mut out = String::new();
        out.push_str(&format!("Trust graph ({} nodes, {} edges)\n", self.nodes.len(), self.edges.len()));
        out.push_str("--------------------------------------------\n");
        out.push_str("peer_id                                    in  out\n");
        for n in nodes {
            let in_d = in_deg.get(n.peer_id.as_str()).unwrap_or(&0);
            let out_d = out_deg.get(n.peer_id.as_str()).unwrap_or(&0);
            let label = n.label.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "{:<42} {:>2}  {:>2}  {}\n",
                truncate(&n.peer_id, 42),
                in_d,
                out_d,
                label
            ));
        }
        // Edges section.
        if !self.edges.is_empty() {
            out.push_str("\nEdges (from -> to):\n");
            for e in &self.edges {
                out.push_str(&format!("  {} -> {}\n", truncate(&e.from, 30), truncate(&e.to, 30)));
            }
        }
        out
    }

    fn render_dot(&self) -> String {
        let mut out = String::new();
        out.push_str("digraph trust {\n");
        out.push_str("  rankdir=LR;\n");
        out.push_str("  node [shape=box, style=rounded];\n");
        for n in &self.nodes {
            let label = n.label.as_deref().unwrap_or(&n.peer_id);
            out.push_str(&format!(
                "  \"{}\" [label=\"{}\\n{:.8}…\"];\n",
                n.peer_id,
                escape_dot(label),
                n.peer_id
            ));
        }
        for e in &self.edges {
            out.push_str(&format!("  \"{}\" -> \"{}\";\n", e.from, e.to));
        }
        out.push_str("}\n");
        out
    }

    /// Returns the "celebrity" peers (top-N by in-degree).
    pub fn celebrities(&self, top: usize) -> Vec<(String, usize)> {
        let mut in_deg: HashMap<&str, usize> = HashMap::new();
        for n in &self.nodes {
            in_deg.insert(&n.peer_id, 0);
        }
        for e in &self.edges {
            *in_deg.entry(&e.to.as_str()).or_insert(0) += 1;
        }
        let mut v: Vec<(String, usize)> = in_deg
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(top);
        v
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_renders_empty() {
        let g = TrustGraph::new();
        let out = g.render(GraphFormat::Ascii);
        assert!(out.contains("empty"));
    }

    #[test]
    fn ascii_renders_node_list() {
        let mut g = TrustGraph::new();
        g.add_node(TrustNode { peer_id: "a".into(), label: None });
        g.add_node(TrustNode { peer_id: "b".into(), label: Some("Alice".into()) });
        g.add_edge(TrustEdge { from: "a".into(), to: "b".into() });
        let out = g.render(GraphFormat::Ascii);
        assert!(out.contains("Trust graph (2 nodes, 1 edges)"));
        assert!(out.contains("Alice"));
    }

    #[test]
    fn dot_renders_digraph() {
        let mut g = TrustGraph::new();
        g.add_node(TrustNode { peer_id: "a".into(), label: None });
        g.add_node(TrustNode { peer_id: "b".into(), label: None });
        g.add_edge(TrustEdge { from: "a".into(), to: "b".into() });
        let out = g.render(GraphFormat::Dot);
        assert!(out.starts_with("digraph trust"));
        assert!(out.contains("\"a\" -> \"b\""));
    }

    #[test]
    fn celebrities_sorted_by_in_degree() {
        let mut g = TrustGraph::new();
        g.add_node(TrustNode { peer_id: "a".into(), label: None });
        g.add_node(TrustNode { peer_id: "b".into(), label: None });
        g.add_node(TrustNode { peer_id: "c".into(), label: None });
        // a trusts c, b trusts c, b trusts a → c has in-degree 2
        g.add_edge(TrustEdge { from: "a".into(), to: "c".into() });
        g.add_edge(TrustEdge { from: "b".into(), to: "c".into() });
        g.add_edge(TrustEdge { from: "b".into(), to: "a".into() });
        let celebs = g.celebrities(3);
        assert_eq!(celebs[0].0, "c");
        assert_eq!(celebs[0].1, 2);
    }

    #[test]
    fn from_signed_by_builds_correctly() {
        let rels = vec![("a".to_string(), "b".to_string()), ("c".to_string(), "b".to_string())];
        let g = TrustGraph::from_signed_by(&rels);
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 2);
    }

    #[test]
    fn add_node_dedupes() {
        let mut g = TrustGraph::new();
        g.add_node(TrustNode { peer_id: "a".into(), label: None });
        g.add_node(TrustNode { peer_id: "a".into(), label: Some("x".into()) });
        assert_eq!(g.nodes.len(), 1);
    }
}
