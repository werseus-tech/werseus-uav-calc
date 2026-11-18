// SPDX-License-Identifier: AGPL-3.0-or-later
//! Mesh-network link budget and connectivity for a formation plus a ground station.
//!
//! - Free-space path loss (Friis): `FSPL[dB] = 20·log10(d[km]) + 20·log10(f[MHz]) + 32.44`
//!   (ITU-R P.525; Rappaport, *Wireless Communications*, §4.2). No terrain, Fresnel-zone,
//!   multipath or interference loss — those are what the fade margin is meant to cover.
//! - Received power `P_rx = P_tx + G_tx + G_rx − FSPL`, margin `P_rx − sensitivity`; a link is
//!   counted as usable when the margin is at least the fade margin.
//! - Hops to the ground station: breadth-first search; single points of failure: articulation
//!   points of the undirected link graph (Hopcroft & Tarjan, CACM 16(6), 1973; Cormen et al.,
//!   *Introduction to Algorithms*, problem 22-2).
//! - Throughput: the usual rough rule for single-radio half-duplex relays, the rate halves at
//!   every additional hop. Approximate only.

/// Radio settings (identical radios on every node).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radio {
    pub freq_mhz: f64,
    pub tx_dbm: f64,
    /// Antenna gain per node [dBi] (used on both ends).
    pub gain_dbi: f64,
    pub sens_dbm: f64,
    pub fade_db: f64,
    /// Nominal single-hop data rate [Mbit/s].
    pub rate_mbps: f64,
}

/// Typical radio presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RadioPreset {
    /// 915 MHz telemetry radio (narrow band, long range, low rate).
    Mhz915,
    /// 2.4 GHz Wi-Fi-class mesh.
    Ghz24,
    /// 5.8 GHz Wi-Fi-class link (video).
    Ghz58,
}

impl RadioPreset {
    pub const ALL: [RadioPreset; 3] = [RadioPreset::Mhz915, RadioPreset::Ghz24, RadioPreset::Ghz58];

    /// "Typical" values, not a specific product.
    pub fn radio(self) -> Radio {
        match self {
            RadioPreset::Mhz915 => Radio {
                freq_mhz: 915.0,
                tx_dbm: 20.0,
                gain_dbi: 2.0,
                sens_dbm: -110.0,
                fade_db: 10.0,
                rate_mbps: 0.25,
            },
            RadioPreset::Ghz24 => Radio {
                freq_mhz: 2400.0,
                tx_dbm: 20.0,
                gain_dbi: 2.0,
                sens_dbm: -90.0,
                fade_db: 10.0,
                rate_mbps: 20.0,
            },
            RadioPreset::Ghz58 => Radio {
                freq_mhz: 5800.0,
                tx_dbm: 20.0,
                gain_dbi: 3.0,
                sens_dbm: -85.0,
                fade_db: 10.0,
                rate_mbps: 50.0,
            },
        }
    }
}

/// Free-space path loss [dB]. Distances below 1 m are treated as 1 m.
pub fn fspl_db(dist_m: f64, freq_mhz: f64) -> f64 {
    let d_km = dist_m.max(1.0) / 1000.0;
    20.0 * d_km.log10() + 20.0 * freq_mhz.max(1e-3).log10() + 32.44
}

/// Link margin above sensitivity [dB] (fade margin not subtracted).
pub fn link_margin_db(r: &Radio, dist_m: f64) -> f64 {
    r.tx_dbm + 2.0 * r.gain_dbi - fspl_db(dist_m, r.freq_mhz) - r.sens_dbm
}

/// Longest distance with margin ≥ fade margin [m].
pub fn max_range_m(r: &Radio) -> f64 {
    let budget = r.tx_dbm + 2.0 * r.gain_dbi - r.sens_dbm - r.fade_db;
    let l_km = (budget - 32.44 - 20.0 * r.freq_mhz.max(1e-3).log10()) / 20.0;
    1000.0 * 10f64.powf(l_km)
}

/// One link between nodes `a < b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub dist: f64,
    pub margin: f64,
    /// Margin ≥ fade margin.
    pub up: bool,
}

/// Mesh analysis result. Node indices: vehicles `0..n−1`, ground station `n`.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshResult {
    pub links: Vec<Link>,
    /// Hops to the ground station (`None` = unreachable), per node.
    pub hops: Vec<Option<usize>>,
    /// Every node reaches the ground station.
    pub connected: bool,
    /// Nodes whose loss disconnects some other node from the ground station's component.
    pub articulation: Vec<usize>,
    /// Approximate throughput to the ground station [Mbit/s], per node.
    pub throughput: Vec<f64>,
}

pub fn adjacency(n: usize, links: &[Link]) -> Vec<Vec<usize>> {
    let mut adj = vec![Vec::new(); n];
    for l in links.iter().filter(|l| l.up) {
        adj[l.a].push(l.b);
        adj[l.b].push(l.a);
    }
    adj
}

/// Breadth-first hop count from `src`.
pub fn bfs_hops(adj: &[Vec<usize>], src: usize) -> Vec<Option<usize>> {
    let mut d = vec![None; adj.len()];
    if src >= adj.len() {
        return d;
    }
    let mut q = std::collections::VecDeque::new();
    d[src] = Some(0);
    q.push_back(src);
    while let Some(u) = q.pop_front() {
        let du = d[u].unwrap_or(0);
        for &v in &adj[u] {
            if d[v].is_none() {
                d[v] = Some(du + 1);
                q.push_back(v);
            }
        }
    }
    d
}

/// Articulation points (cut vertices) of an undirected graph, sorted. Iterative Tarjan DFS
/// (low-link values) so deep graphs cannot overflow the stack.
pub fn articulation_points(adj: &[Vec<usize>]) -> Vec<usize> {
    let n = adj.len();
    let mut disc = vec![usize::MAX; n];
    let mut low = vec![0usize; n];
    let mut parent = vec![usize::MAX; n];
    let mut is_ap = vec![false; n];
    let mut timer = 0usize;
    for root in 0..n {
        if disc[root] != usize::MAX {
            continue;
        }
        let mut root_children = 0usize;
        // Stack of (node, next neighbour index).
        let mut stack = vec![(root, 0usize)];
        disc[root] = timer;
        low[root] = timer;
        timer += 1;
        while let Some(&mut (u, ref mut it)) = stack.last_mut() {
            if *it < adj[u].len() {
                let v = adj[u][*it];
                *it += 1;
                if disc[v] == usize::MAX {
                    parent[v] = u;
                    if u == root {
                        root_children += 1;
                    }
                    disc[v] = timer;
                    low[v] = timer;
                    timer += 1;
                    stack.push((v, 0));
                } else if v != parent[u] {
                    low[u] = low[u].min(disc[v]);
                }
            } else {
                stack.pop();
                let p = parent[u];
                if p != usize::MAX {
                    low[p] = low[p].min(low[u]);
                    if p != root && low[u] >= disc[p] {
                        is_ap[p] = true;
                    }
                }
            }
        }
        if root_children > 1 {
            is_ap[root] = true;
        }
    }
    (0..n).filter(|&i| is_ap[i]).collect()
}

/// Full analysis. `nodes`: positions `[x, y, h]`; the last one is the ground station.
pub fn analyse(nodes: &[[f64; 3]], r: &Radio) -> MeshResult {
    let n = nodes.len();
    let mut links = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            let dist = super::formation::dist(nodes[a], nodes[b]);
            let margin = link_margin_db(r, dist);
            links.push(Link {
                a,
                b,
                dist,
                margin,
                up: margin.is_finite() && margin >= r.fade_db,
            });
        }
    }
    let adj = adjacency(n, &links);
    let hops = if n > 0 {
        bfs_hops(&adj, n - 1)
    } else {
        Vec::new()
    };
    let connected = hops.iter().all(|h| h.is_some());
    // Yalnızca yer istasyonuna ulaşabilen araçlar; yer istasyonunun kendisi hariç.
    let articulation: Vec<usize> = articulation_points(&adj)
        .into_iter()
        .filter(|&i| i + 1 != n && hops[i].is_some())
        .collect();
    let throughput = hops
        .iter()
        .map(|h| match h {
            Some(0) => r.rate_mbps,
            Some(k) => r.rate_mbps * 0.5f64.powi(*k as i32 - 1),
            None => 0.0,
        })
        .collect();
    MeshResult {
        links,
        hops,
        connected,
        articulation,
        throughput,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
        let mut adj = vec![Vec::new(); n];
        for &(a, b) in edges {
            adj[a].push(b);
            adj[b].push(a);
        }
        adj
    }

    #[test]
    fn fspl_and_margin_hand_values() {
        // 1 km at 2400 MHz: 20·log10(2400) + 32.44 = 100.04 dB.
        assert!((fspl_db(1000.0, 2400.0) - 100.044).abs() < 0.01);
        // Doubling distance adds 6.02 dB.
        assert!((fspl_db(2000.0, 915.0) - fspl_db(1000.0, 915.0) - 6.0206).abs() < 1e-3);
        let r = RadioPreset::Ghz24.radio();
        // 20 + 4 − 100.04 + 90 = 13.96 dB.
        assert!((link_margin_db(&r, 1000.0) - 13.956).abs() < 0.01);
        let d = max_range_m(&r);
        assert!((link_margin_db(&r, d) - r.fade_db).abs() < 1e-9);
        assert!(max_range_m(&RadioPreset::Mhz915.radio()) > d);
        assert!(max_range_m(&RadioPreset::Ghz58.radio()) < d);
    }

    #[test]
    fn bfs_hops_on_a_path_and_a_star() {
        let path = graph(4, &[(0, 1), (1, 2), (2, 3)]);
        assert_eq!(bfs_hops(&path, 3), vec![Some(3), Some(2), Some(1), Some(0)]);
        let split = graph(4, &[(0, 1), (2, 3)]);
        assert_eq!(bfs_hops(&split, 3), vec![None, None, Some(1), Some(0)]);
        let star = graph(4, &[(3, 0), (3, 1), (3, 2)]);
        assert_eq!(bfs_hops(&star, 3), vec![Some(1), Some(1), Some(1), Some(0)]);
    }

    #[test]
    fn articulation_points_on_hand_graphs() {
        // Path 0-1-2-3: inner nodes are cut vertices.
        assert_eq!(
            articulation_points(&graph(4, &[(0, 1), (1, 2), (2, 3)])),
            vec![1, 2]
        );
        // Cycle: none.
        assert!(articulation_points(&graph(4, &[(0, 1), (1, 2), (2, 3), (3, 0)])).is_empty());
        // Two triangles sharing node 2 (bow tie).
        assert_eq!(
            articulation_points(&graph(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)])),
            vec![2]
        );
        // Star: centre only (centre is the DFS root here).
        assert_eq!(
            articulation_points(&graph(4, &[(0, 1), (0, 2), (0, 3)])),
            vec![0]
        );
        // Disconnected graph with a bridge in one part.
        assert_eq!(
            articulation_points(&graph(5, &[(0, 1), (1, 2), (3, 4)])),
            vec![1]
        );
    }

    #[test]
    fn analyse_relay_chain_halves_throughput() {
        let r = RadioPreset::Ghz24.radio();
        let step = 0.8 * max_range_m(&r);
        // GS at the end of a straight chain: vehicle 0 three hops away.
        let nodes = [
            [0.0, 0.0, 0.0],
            [step, 0.0, 0.0],
            [2.0 * step, 0.0, 0.0],
            [3.0 * step, 0.0, 0.0],
        ];
        let m = analyse(&nodes, &r);
        assert!(m.connected);
        assert_eq!(m.hops, vec![Some(3), Some(2), Some(1), Some(0)]);
        assert_eq!(m.articulation, vec![1, 2]);
        assert!((m.throughput[0] - r.rate_mbps / 4.0).abs() < 1e-12);
        assert!((m.throughput[2] - r.rate_mbps).abs() < 1e-12);
        // Pull the last vehicle out of range.
        let mut far = nodes;
        far[0][0] = -3.0 * step;
        let m = analyse(&far, &r);
        assert!(!m.connected);
        assert_eq!(m.throughput[0], 0.0);
        // Yer istasyonuna ulaşamayan kümedeki kesme noktaları ve yer istasyonu raporlanmaz.
        let split = [
            [-20.0 * step, 0.0, 0.0],
            [-20.0 * step + 0.8 * step, 0.0, 0.0],
            [-20.0 * step + 1.6 * step, 0.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let m2 = analyse(&split, &r);
        assert!(!m2.connected);
        assert!(m2.articulation.is_empty(), "{:?}", m2.articulation);
    }
}
