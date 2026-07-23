//! Shared input canonicalization for every layout pass.
//!
//! One spine, three passes: id resolution, duplicate-node policy,
//! unresolvable-edge policy, self-edge extraction, DFS cycle breaking
//! and weakly-connected components live here so the honesty behavior is
//! identical whichever algorithm the consumer selects.

use std::collections::HashMap;

use abstracttui::base::Size;

use crate::desc::GraphDesc;

/// One resolvable, non-self edge in local (kept-node) indices.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedEdge {
    pub from: usize,
    pub to: usize,
    /// Index into `GraphDesc::edges`.
    pub desc_index: usize,
    /// Set by [`Resolved::break_cycles`]: this edge is treated as
    /// reversed (to -> from) so the graph ranks as a DAG.
    pub broken: bool,
}

/// Canonicalized graph: kept nodes (first occurrence of each id wins),
/// clamped sizes, resolvable edges split from self-edges, and the drop
/// counts that feed the layout's honesty label.
pub(crate) struct Resolved {
    /// For each kept node: its index into `GraphDesc::nodes`.
    pub desc_index: Vec<usize>,
    /// For each kept node: its card size, clamped to at least 1x1.
    pub sizes: Vec<Size>,
    /// Resolvable edges between distinct kept nodes, in input order.
    pub edges: Vec<ResolvedEdge>,
    /// Self-edges as (local node, desc edge index), in input order.
    pub self_edges: Vec<(usize, usize)>,
    dropped_nodes: usize,
    skipped_edges: usize,
}

impl Resolved {
    pub fn new(desc: &GraphDesc) -> Self {
        // Lookup-only map (never iterated: HashMap iteration order would
        // break the determinism contract).
        let mut by_id: HashMap<&str, usize> = HashMap::with_capacity(desc.nodes.len());
        let mut desc_index = Vec::new();
        let mut sizes = Vec::new();
        let mut dropped_nodes = 0usize;
        for (i, node) in desc.nodes.iter().enumerate() {
            if by_id.contains_key(node.id.as_str()) {
                dropped_nodes += 1;
                continue;
            }
            by_id.insert(node.id.as_str(), desc_index.len());
            desc_index.push(i);
            sizes.push(Size::new(node.size.w.max(1), node.size.h.max(1)));
        }

        let mut edges = Vec::new();
        let mut self_edges = Vec::new();
        let mut skipped_edges = 0usize;
        for (i, edge) in desc.edges.iter().enumerate() {
            match (by_id.get(edge.from.as_str()), by_id.get(edge.to.as_str())) {
                (Some(&f), Some(&t)) if f == t => self_edges.push((f, i)),
                (Some(&f), Some(&t)) => edges.push(ResolvedEdge {
                    from: f,
                    to: t,
                    desc_index: i,
                    broken: false,
                }),
                _ => skipped_edges += 1,
            }
        }

        Resolved {
            desc_index,
            sizes,
            edges,
            self_edges,
            dropped_nodes,
            skipped_edges,
        }
    }

    /// Number of kept nodes.
    pub fn len(&self) -> usize {
        self.desc_index.len()
    }

    /// The node id of local index `i`.
    pub fn id<'a>(&self, desc: &'a GraphDesc, i: usize) -> &'a str {
        &desc.nodes[self.desc_index[i]].id
    }

    /// Honesty notes for the input canonicalization (empty when the
    /// input was clean). Passes fold these into `Layout::fallback`.
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if self.dropped_nodes > 0 {
            notes.push(format!(
                "{} duplicate node id(s) dropped (first occurrence wins)",
                self.dropped_nodes
            ));
        }
        if self.skipped_edges > 0 {
            notes.push(format!(
                "{} edge(s) skipped (unknown endpoint id)",
                self.skipped_edges
            ));
        }
        notes
    }

    /// Break cycles with the documented heuristic: a depth-first search
    /// in input node/edge order marks every back edge (an edge into a
    /// node currently on the DFS stack) as `broken`. Treating broken
    /// edges as reversed yields an acyclic graph (the classic DFS
    /// back-edge result); the layered tests pin the consequence — ranks
    /// strictly increase along every non-broken edge.
    pub fn break_cycles(&mut self) {
        let n = self.len();
        let mut out: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for (pos, e) in self.edges.iter().enumerate() {
            out[e.from].push((e.to, pos));
        }

        #[derive(Copy, Clone, PartialEq)]
        enum State {
            White,
            Gray,
            Black,
        }
        let mut state = vec![State::White; n];
        // Iterative DFS: (node, next out-edge position) — recursion depth
        // is caller data at 500+ nodes, so no call stack.
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for root in 0..n {
            if state[root] != State::White {
                continue;
            }
            state[root] = State::Gray;
            stack.push((root, 0));
            while let Some(&mut (u, ref mut next)) = stack.last_mut() {
                if *next < out[u].len() {
                    let (v, pos) = out[u][*next];
                    *next += 1;
                    match state[v] {
                        State::White => {
                            state[v] = State::Gray;
                            stack.push((v, 0));
                        }
                        State::Gray => self.edges[pos].broken = true,
                        State::Black => {}
                    }
                } else {
                    state[u] = State::Black;
                    stack.pop();
                }
            }
        }
    }

    /// Weakly-connected components over kept nodes (self-edges do not
    /// connect anything new). Returns the component id of every node;
    /// ids are assigned in input-order discovery, so component 0 holds
    /// the first node.
    pub fn components(&self) -> Vec<usize> {
        let n = self.len();
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
        for e in &self.edges {
            adj[e.from].push(e.to);
            adj[e.to].push(e.from);
        }
        let mut comp = vec![usize::MAX; n];
        let mut next = 0usize;
        let mut queue = Vec::new();
        for root in 0..n {
            if comp[root] != usize::MAX {
                continue;
            }
            comp[root] = next;
            queue.push(root);
            while let Some(u) = queue.pop() {
                for &v in &adj[u] {
                    if comp[v] == usize::MAX {
                        comp[v] = next;
                        queue.push(v);
                    }
                }
            }
            next += 1;
        }
        comp
    }
}

/// Join honesty notes into the single `Layout::fallback` label.
pub(crate) fn fold_notes(notes: Vec<String>) -> Option<String> {
    if notes.is_empty() {
        None
    } else {
        Some(notes.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::GraphDesc;

    fn abc_cycle() -> GraphDesc {
        GraphDesc::new()
            .node("a", 4, 2)
            .node("b", 4, 2)
            .node("c", 4, 2)
            .edge("a", "b")
            .edge("b", "c")
            .edge("c", "a")
    }

    #[test]
    fn duplicates_and_unknowns_are_counted_not_silent() {
        let desc = GraphDesc::new()
            .node("a", 4, 2)
            .node("a", 9, 9)
            .node("b", 0, -3)
            .edge("a", "b")
            .edge("a", "ghost")
            .edge("a", "a");
        let r = Resolved::new(&desc);
        assert_eq!(r.len(), 2);
        assert_eq!(r.sizes[1], Size::new(1, 1), "degenerate sizes clamp");
        assert_eq!(r.edges.len(), 1);
        assert_eq!(r.self_edges, vec![(0, 2)]);
        let notes = r.notes();
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("duplicate node id"));
        assert!(notes[1].contains("unknown endpoint"));
    }

    #[test]
    fn dfs_breaks_exactly_the_back_edge() {
        let mut r = Resolved::new(&abc_cycle());
        r.break_cycles();
        let broken: Vec<usize> = r
            .edges
            .iter()
            .filter(|e| e.broken)
            .map(|e| e.desc_index)
            .collect();
        // Input-order DFS a -> b -> c sees c->a as the back edge.
        assert_eq!(broken, vec![2]);
    }

    #[test]
    fn components_follow_input_order() {
        let desc = GraphDesc::new()
            .node("x", 4, 2)
            .node("a", 4, 2)
            .node("b", 4, 2)
            .edge("a", "b");
        let r = Resolved::new(&desc);
        assert_eq!(r.components(), vec![0, 1, 1]);
    }
}
