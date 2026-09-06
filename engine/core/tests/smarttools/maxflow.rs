use calumma_core::smarttools::maxflow::FlowGraph;
use proptest::prelude::*;

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-3, "{a} != {b}");
}

fn edmonds_karp(n: usize, edges: &[(usize, usize, f32)], s: usize, t: usize) -> f32 {
    let mut cap = vec![vec![0.0f32; n]; n];
    for &(u, v, c) in edges {
        cap[u][v] += c;
    }
    let mut flow = 0.0;
    loop {
        let mut parent = vec![None; n];
        let mut q = std::collections::VecDeque::new();
        q.push_back(s);
        parent[s] = Some(s);
        while let Some(v) = q.pop_front() {
            if v == t {
                break;
            }
            for u in 0..n {
                if parent[u].is_none() && cap[v][u] > 1e-6 {
                    parent[u] = Some(v);
                    q.push_back(u);
                }
            }
        }
        if parent[t].is_none() {
            break;
        }
        let mut bottleneck = f32::INFINITY;
        let mut v = t;
        while v != s {
            let u = parent[v].unwrap();
            bottleneck = bottleneck.min(cap[u][v]);
            v = u;
        }
        v = t;
        while v != s {
            let u = parent[v].unwrap();
            cap[u][v] -= bottleneck;
            cap[v][u] += bottleneck;
            v = u;
        }
        flow += bottleneck;
    }
    flow
}

#[test]
fn a_single_edge_carries_its_own_capacity() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 1, 5.0, 0.0);
    approx(g.max_flow(0, 1), 5.0);
}

#[test]
fn a_chain_is_limited_by_its_narrowest_link() {
    let mut g = FlowGraph::new(4);
    g.add_edge(0, 1, 10.0, 0.0);
    g.add_edge(1, 2, 3.0, 0.0);
    g.add_edge(2, 3, 7.0, 0.0);
    approx(g.max_flow(0, 3), 3.0);
}

#[test]
fn parallel_paths_add_up() {
    let mut g = FlowGraph::new(4);
    g.add_edge(0, 1, 4.0, 0.0);
    g.add_edge(1, 3, 4.0, 0.0);
    g.add_edge(0, 2, 6.0, 0.0);
    g.add_edge(2, 3, 6.0, 0.0);
    approx(g.max_flow(0, 3), 10.0);
}

#[test]
fn a_disconnected_sink_carries_nothing() {
    let mut g = FlowGraph::new(3);
    g.add_edge(0, 1, 9.0, 0.0);
    approx(g.max_flow(0, 2), 0.0);
}

#[test]
fn the_clrs_example_network_has_a_max_flow_of_twenty_three() {
    let mut g = FlowGraph::new(6);
    g.add_edge(0, 1, 16.0, 0.0);
    g.add_edge(0, 2, 13.0, 0.0);
    g.add_edge(1, 2, 10.0, 0.0);
    g.add_edge(2, 1, 4.0, 0.0);
    g.add_edge(1, 3, 12.0, 0.0);
    g.add_edge(3, 2, 9.0, 0.0);
    g.add_edge(2, 4, 14.0, 0.0);
    g.add_edge(4, 3, 7.0, 0.0);
    g.add_edge(3, 5, 20.0, 0.0);
    g.add_edge(4, 5, 4.0, 0.0);
    approx(g.max_flow(0, 5), 23.0);
}

#[test]
fn the_min_cut_splits_the_graph_where_the_bottleneck_is() {
    let mut g = FlowGraph::new(4);
    g.add_edge(0, 1, 10.0, 0.0);
    g.add_edge(1, 2, 1.0, 0.0);
    g.add_edge(2, 3, 10.0, 0.0);
    approx(g.max_flow(0, 3), 1.0);
    let side = g.source_side(0);
    assert!(
        side[0] && side[1],
        "upstream of the bottleneck is source-side"
    );
    assert!(!side[2] && !side[3], "downstream of it is sink-side");
}

#[test]
fn an_undirected_edge_carries_its_capacity_once() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 1, 4.0, 4.0);
    approx(g.max_flow(0, 1), 4.0);
}

#[test]
fn a_terminal_plus_neighbour_grid_cuts_along_the_cheap_seam() {
    let (s, t) = (4, 5);
    let mut g = FlowGraph::new(6);
    for p in [0, 1] {
        g.add_edge(s, p, 10.0, 0.0);
        g.add_edge(p, t, 1.0, 0.0);
    }
    for p in [2, 3] {
        g.add_edge(s, p, 1.0, 0.0);
        g.add_edge(p, t, 10.0, 0.0);
    }
    g.add_edge(0, 1, 5.0, 5.0);
    g.add_edge(2, 3, 5.0, 5.0);
    g.add_edge(1, 2, 0.5, 0.5);
    g.max_flow(s, t);
    let side = g.source_side(s);
    assert!(
        side[0] && side[1],
        "the foreground-looking pair stays with the source"
    );
    assert!(
        !side[2] && !side[3],
        "the background-looking pair goes to the sink"
    );
}

#[test]
fn a_strong_neighbour_link_overrules_a_weak_terminal_preference() {
    let (s, t) = (2, 3);
    let mut g = FlowGraph::new(4);
    g.add_edge(s, 0, 10.0, 0.0);
    g.add_edge(0, t, 1.0, 0.0);
    g.add_edge(s, 1, 1.0, 0.0);
    g.add_edge(1, t, 2.0, 0.0);
    g.add_edge(0, 1, 50.0, 50.0);
    g.max_flow(s, t);
    let side = g.source_side(s);
    assert!(side[0]);
    assert!(
        side[1],
        "the tie to pixel 0 outweighs its own mild preference"
    );
}

#[test]
fn a_very_long_chain_does_not_overflow_the_stack() {
    let n = 60_000;
    let mut g = FlowGraph::with_capacity(n, n);
    for i in 0..n - 1 {
        g.add_edge(i, i + 1, 2.0, 0.0);
    }
    approx(g.max_flow(0, n - 1), 2.0);
}

#[test]
fn source_equals_sink_is_zero_flow() {
    let mut g = FlowGraph::new(3);
    g.add_edge(0, 1, 4.0, 0.0);
    g.add_edge(1, 2, 4.0, 0.0);
    approx(g.max_flow(1, 1), 0.0);
}

#[test]
fn node_count_matches_construction() {
    let g = FlowGraph::with_capacity(12, 40);
    assert_eq!(g.node_count(), 12);
}

#[test]
fn reset_flow_restores_the_original_capacities() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 1, 5.0, 0.0);
    approx(g.max_flow(0, 1), 5.0);
    approx(g.max_flow(0, 1), 0.0);
    g.reset_flow();
    approx(g.max_flow(0, 1), 5.0);
}

#[test]
fn set_capacity_changes_what_the_next_flow_can_push() {
    let mut g = FlowGraph::new(2);
    let e = g.add_edge(0, 1, 9.0, 0.0);
    g.set_capacity(e, 2.0, 0.0);
    approx(g.max_flow(0, 1), 2.0);
    g.reset_flow();
    approx(g.max_flow(0, 1), 2.0);
}

#[test]
fn a_diamond_network_matches_edmonds_karp() {
    let edges = [
        (0, 1, 5.0),
        (0, 2, 5.0),
        (1, 3, 3.0),
        (2, 3, 7.0),
        (1, 2, 10.0),
    ];
    let mut g = FlowGraph::new(4);
    for &(u, v, c) in &edges {
        g.add_edge(u, v, c, 0.0);
    }
    let got = g.max_flow(0, 3);
    approx(got, edmonds_karp(4, &edges, 0, 3));
    approx(got, 10.0);
}

#[test]
fn a_four_cycle_with_a_chord_matches_edmonds_karp() {
    let edges = [
        (0, 1, 4.0),
        (1, 3, 4.0),
        (0, 2, 2.0),
        (2, 3, 2.0),
        (1, 2, 1.0),
    ];
    let mut g = FlowGraph::new(4);
    for &(u, v, c) in &edges {
        g.add_edge(u, v, c, 0.0);
    }
    approx(g.max_flow(0, 3), edmonds_karp(4, &edges, 0, 3));
}

#[test]
fn source_side_before_any_flow_reaches_everyone_connected() {
    let mut g = FlowGraph::new(3);
    g.add_edge(0, 1, 1.0, 0.0);
    g.add_edge(1, 2, 1.0, 0.0);
    let side = g.source_side(0);
    assert!(side[0] && side[1] && side[2]);
}

#[test]
fn zero_capacity_edges_carry_nothing() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 1, 0.0, 0.0);
    approx(g.max_flow(0, 1), 0.0);
}

#[test]
fn negative_and_non_finite_capacities_are_treated_as_zero() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 1, -4.0, 0.0);
    approx(g.max_flow(0, 1), 0.0);
    g.reset_flow();
    let e = g.add_edge(0, 1, f32::NAN, f32::INFINITY);
    approx(g.max_flow(0, 1), 0.0);
    g.set_capacity(e, -1.0, f32::NEG_INFINITY);
    g.reset_flow();
    approx(g.max_flow(0, 1), 0.0);
}

#[test]
fn huge_capacities_do_not_poison_the_solver() {
    let mut g = FlowGraph::new(3);
    g.add_edge(0, 1, 1.0e30, 0.0);
    g.add_edge(1, 2, 3.0, 0.0);
    approx(g.max_flow(0, 2), 3.0);
}

#[test]
fn resetting_after_set_capacity_keeps_the_new_value() {
    let mut g = FlowGraph::new(3);
    let e = g.add_edge(0, 1, 8.0, 0.0);
    g.add_edge(1, 2, 8.0, 0.0);
    approx(g.max_flow(0, 2), 8.0);
    g.set_capacity(e, 2.0, 0.0);
    g.reset_flow();
    approx(g.max_flow(0, 2), 2.0);
}

proptest! {
    #[test]
    fn random_dags_match_edmonds_karp(
        n in 3usize..8,
        seed in 1u64..10_000,
    ) {
        let mut edges = Vec::new();
        let mut x = seed;
        for u in 0..n {
            for v in (u + 1)..n {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                if x % 3 != 0 {
                    let cap = ((x % 9) + 1) as f32;
                    edges.push((u, v, cap));
                }
            }
        }
        if edges.is_empty() {
            return Ok(());
        }
        let mut g = FlowGraph::new(n);
        for &(u, v, c) in &edges {
            g.add_edge(u, v, c, 0.0);
        }
        let got = g.max_flow(0, n - 1);
        let expect = edmonds_karp(n, &edges, 0, n - 1);
        prop_assert!((got - expect).abs() < 1e-2, "{got} != {expect}");
    }
}

#[test]
fn parallel_edges_add_their_capacities() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 1, 3.0, 0.0);
    g.add_edge(0, 1, 4.0, 0.0);
    approx(g.max_flow(0, 1), 7.0);
}

#[test]
fn source_side_after_reset_reaches_everyone_connected() {
    let mut g = FlowGraph::new(3);
    g.add_edge(0, 1, 1.0, 0.0);
    g.add_edge(1, 2, 1.0, 0.0);
    approx(g.max_flow(0, 2), 1.0);
    g.reset_flow();
    let side = g.source_side(0);
    assert!(side[0] && side[1] && side[2]);
}

#[test]
fn a_self_loop_does_not_create_flow_to_the_sink() {
    let mut g = FlowGraph::new(2);
    g.add_edge(0, 0, 9.0, 0.0);
    g.add_edge(0, 1, 2.0, 0.0);
    approx(g.max_flow(0, 1), 2.0);
}

#[test]
fn a_disconnected_source_carries_nothing() {
    let mut g = FlowGraph::new(3);
    g.add_edge(1, 2, 9.0, 0.0);
    approx(g.max_flow(0, 2), 0.0);
    let side = g.source_side(0);
    assert!(side[0]);
    assert!(!side[1] && !side[2]);
}
