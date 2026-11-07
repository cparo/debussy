//! Finding the graph's cycles and the order to run it in
//! (`docs/architecture/signal-graph.md` "Feedback loops"). Runs off the real-time thread.
//!
//! Nodes are numbered `0..n` and connections are `(from, to)` pairs. The strongly connected
//! components are found with Kosaraju's algorithm, whose second pass yields them in topological
//! order. Within a component that is a cycle, a connection into a feedback delay of the same
//! component *closes the loop*; with those removed the component must be acyclic, or some cycle
//! passes through no feedback delay.

use crate::error::Error;

/// One step of the run order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Group {
    /// A node outside any cycle, run once per block.
    Single(usize),
    /// A cycle, run in sub-blocks: its feedback delays, then its other nodes in this order.
    Cycle {
        /// The cycle's feedback delays.
        delays: Vec<usize>,
        /// The cycle's other nodes, in an order that respects every connection that does not
        /// close the loop.
        members: Vec<usize>,
    },
}

/// The run order of a graph and which connections close loops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Analysis {
    /// Every node in exactly one group, the groups in topological order.
    pub(crate) groups: Vec<Group>,
    /// For each connection, whether it closes a loop.
    pub(crate) closes_loop: Vec<bool>,
}

/// Orders `n` nodes joined by `edges`; `is_delay[i]` marks feedback delays.
///
/// # Errors
///
/// [`Error::CycleWithoutDelay`] if some cycle passes through no feedback delay.
pub(crate) fn analyse(
    n: usize,
    edges: &[(usize, usize)],
    is_delay: &[bool],
) -> Result<Analysis, Error> {
    let mut forward = vec![Vec::new(); n];
    let mut backward = vec![Vec::new(); n];
    for &(from, to) in edges {
        forward[from].push(to);
        backward[to].push(from);
    }

    let finish = finish_order(&forward);
    let mut component = vec![usize::MAX; n];
    let mut components: Vec<Vec<usize>> = Vec::new();
    for &root in finish.iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        let index = components.len();
        let mut members = Vec::new();
        let mut stack = vec![root];
        component[root] = index;
        while let Some(node) = stack.pop() {
            members.push(node);
            for &prev in &backward[node] {
                if component[prev] == usize::MAX {
                    component[prev] = index;
                    stack.push(prev);
                }
            }
        }
        members.sort_unstable();
        components.push(members);
    }

    // A component is a cycle if it has several nodes or one node connected to itself.
    let mut cyclic: Vec<bool> = components.iter().map(|members| members.len() > 1).collect();
    for &(from, to) in edges {
        if from == to {
            cyclic[component[from]] = true;
        }
    }
    let closes_loop: Vec<bool> = edges
        .iter()
        .map(|&(from, to)| is_delay[to] && component[from] == component[to])
        .collect();

    let mut groups = Vec::with_capacity(components.len());
    for (index, members) in components.into_iter().enumerate() {
        if !cyclic[index] {
            groups.push(Group::Single(members[0]));
            continue;
        }
        let order = order_within(&members, edges, &closes_loop, &component, index)?;
        let (delays, members) = order.into_iter().partition(|&node| is_delay[node]);
        groups.push(Group::Cycle { delays, members });
    }
    Ok(Analysis {
        groups,
        closes_loop,
    })
}

/// The order in which an iterative depth-first search over `forward` finishes each node.
fn finish_order(forward: &[Vec<usize>]) -> Vec<usize> {
    let n = forward.len();
    let mut visited = vec![false; n];
    let mut finish = Vec::with_capacity(n);
    for root in 0..n {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        let mut stack = vec![(root, 0_usize)];
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if let Some(&child) = forward[node].get(*next) {
                *next += 1;
                if !visited[child] {
                    visited[child] = true;
                    stack.push((child, 0));
                }
            } else {
                finish.push(node);
                stack.pop();
            }
        }
    }
    finish
}

/// Orders the members of component `index` by the connections inside it that do not close a
/// loop (Kahn's algorithm, smallest node first).
fn order_within(
    members: &[usize],
    edges: &[(usize, usize)],
    closes_loop: &[bool],
    component: &[usize],
    index: usize,
) -> Result<Vec<usize>, Error> {
    let inside: Vec<(usize, usize)> = edges
        .iter()
        .zip(closes_loop)
        .filter(|&(&(from, to), &closes)| {
            !closes && component[from] == index && component[to] == index
        })
        .map(|(&edge, _)| edge)
        .collect();
    let mut waiting: Vec<(usize, usize)> = members
        .iter()
        .map(|&node| (node, inside.iter().filter(|&&(_, to)| to == node).count()))
        .collect();
    let mut order = Vec::with_capacity(members.len());
    while let Some(position) = waiting.iter().position(|&(_, count)| count == 0) {
        let (node, _) = waiting.remove(position);
        order.push(node);
        for &(_, to) in inside.iter().filter(|&&(from, _)| from == node) {
            if let Some(entry) = waiting.iter_mut().find(|(n, _)| *n == to) {
                entry.1 -= 1;
            }
        }
    }
    if waiting.is_empty() {
        Ok(order)
    } else {
        Err(Error::CycleWithoutDelay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyse_orders_a_chain() -> Result<(), Error> {
        let analysis = analyse(3, &[(2, 0), (0, 1)], &[false; 3])?;
        assert_eq!(
            analysis.groups,
            [Group::Single(2), Group::Single(0), Group::Single(1)]
        );
        assert_eq!(analysis.closes_loop, [false, false]);
        Ok(())
    }

    #[test]
    fn test_analyse_refuses_cycle_without_delay() {
        assert_eq!(
            analyse(2, &[(0, 1), (1, 0)], &[false; 2]),
            Err(Error::CycleWithoutDelay)
        );
        assert_eq!(
            analyse(1, &[(0, 0)], &[false]),
            Err(Error::CycleWithoutDelay)
        );
    }

    #[test]
    fn test_analyse_refuses_delay_free_loop_beside_delayed_one() {
        // 0 ⇄ 1 has no delay, though 0 → 2 (delay) → 0 does.
        let edges = [(0, 1), (1, 0), (0, 2), (2, 0)];
        assert_eq!(
            analyse(3, &edges, &[false, false, true]),
            Err(Error::CycleWithoutDelay)
        );
    }

    #[test]
    fn test_analyse_groups_a_delayed_cycle() -> Result<(), Error> {
        // 0 → 1 → 2 (delay) → 1, then 1 → 3.
        let edges = [(0, 1), (1, 2), (2, 1), (1, 3)];
        let analysis = analyse(4, &edges, &[false, false, true, false])?;
        assert_eq!(
            analysis.groups,
            [
                Group::Single(0),
                Group::Cycle {
                    delays: vec![2],
                    members: vec![1]
                },
                Group::Single(3),
            ]
        );
        assert_eq!(analysis.closes_loop, [false, true, false, false]);
        Ok(())
    }

    #[test]
    fn test_analyse_accepts_delay_self_loop() -> Result<(), Error> {
        let analysis = analyse(1, &[(0, 0)], &[true])?;
        assert_eq!(
            analysis.groups,
            [Group::Cycle {
                delays: vec![0],
                members: vec![]
            }]
        );
        Ok(())
    }
}
