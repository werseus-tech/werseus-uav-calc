// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formation geometry and the collision / spacing check.
//!
//! Frame used by the whole `swarm` module: `x` points **aft** (downstream, the way the wake is
//! carried), `y` to starboard (right), `h` up. All lengths in metres. Vehicle 0 is the leader.
//!
//! The spacing check is the usual "keep-out bubble" budget used in drone-show and formation
//! planning: two vehicles must stay at least one vehicle size apart *plus* the position error of
//! each of them (both can be off in opposite directions), i.e. `d_req = L + 2·ε`.

/// Formation shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormationKind {
    /// Line abreast (side by side).
    Line,
    /// Line astern / column (in trail, each one `step_x` behind and `step_h` below).
    Column,
    /// V: leader at the apex, ranks alternate left / right.
    Vee,
    /// Echelon right: every vehicle one step aft and one step right of the previous one.
    Echelon,
    /// Rectangular grid (as square as possible), row spacing = column spacing.
    Grid,
    /// Ring with equal chord between neighbours.
    Circle,
}

impl FormationKind {
    pub const ALL: [FormationKind; 6] = [
        FormationKind::Line,
        FormationKind::Column,
        FormationKind::Vee,
        FormationKind::Echelon,
        FormationKind::Grid,
        FormationKind::Circle,
    ];
}

/// Formation inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FormationParams {
    pub kind: FormationKind,
    /// Number of vehicles (≥ 1).
    pub n: usize,
    /// Lateral spacing between neighbours, centre to centre [m] (also grid / ring spacing).
    pub spacing: f64,
    /// Streamwise step per rank [m] (V, echelon, column).
    pub step_x: f64,
    /// Vertical step per rank [m] (positive: followers lower). V, echelon, column, grid rows.
    pub step_h: f64,
}

/// Vehicle centre positions `[x_aft, y_right, h_up]`.
pub fn positions(fp: &FormationParams) -> Vec<[f64; 3]> {
    let n = fp.n.max(1);
    let s = finite_or(fp.spacing, 0.0).max(0.0);
    let sx = finite_or(fp.step_x, 0.0);
    let sh = finite_or(fp.step_h, 0.0);
    let mut out = Vec::with_capacity(n);
    match fp.kind {
        FormationKind::Line => {
            let y0 = -0.5 * s * (n as f64 - 1.0);
            for i in 0..n {
                out.push([0.0, y0 + s * i as f64, 0.0]);
            }
        }
        FormationKind::Column => {
            for i in 0..n {
                let k = i as f64;
                out.push([k * sx, 0.0, -k * sh]);
            }
        }
        FormationKind::Vee => {
            out.push([0.0, 0.0, 0.0]);
            for i in 1..n {
                let rank = i.div_ceil(2) as f64;
                let side = if i % 2 == 1 { -1.0 } else { 1.0 };
                out.push([rank * sx, side * rank * s, -rank * sh]);
            }
        }
        FormationKind::Echelon => {
            for i in 0..n {
                let k = i as f64;
                out.push([k * sx, k * s, -k * sh]);
            }
        }
        FormationKind::Grid => {
            let cols = (n as f64).sqrt().ceil().max(1.0) as usize;
            let y0 = -0.5 * s * (cols as f64 - 1.0);
            for i in 0..n {
                let r = (i / cols) as f64;
                let c = (i % cols) as f64;
                out.push([r * s, y0 + c * s, -r * sh]);
            }
        }
        FormationKind::Circle => {
            if n == 1 {
                out.push([0.0, 0.0, 0.0]);
            } else {
                let r = s / (2.0 * (std::f64::consts::PI / n as f64).sin());
                for i in 0..n {
                    let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                    // Vehicle 0 at the front of the ring.
                    out.push([-r * a.cos(), r * a.sin(), 0.0]);
                }
            }
        }
    }
    out
}

fn finite_or(x: f64, d: f64) -> f64 {
    if x.is_finite() { x } else { d }
}

/// 3D distance.
pub fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Largest distance of any vehicle from the formation centroid (×2 = "diameter").
pub fn extent(pos: &[[f64; 3]]) -> f64 {
    if pos.is_empty() {
        return 0.0;
    }
    let c = centroid(pos);
    2.0 * pos.iter().map(|p| dist(*p, c)).fold(0.0, f64::max)
}

pub fn centroid(pos: &[[f64; 3]]) -> [f64; 3] {
    let n = pos.len().max(1) as f64;
    let mut c = [0.0; 3];
    for p in pos {
        for k in 0..3 {
            c[k] += p[k] / n;
        }
    }
    c
}

/// Closest pair `(i, j, distance)`; `None` for fewer than two vehicles.
pub fn min_pair(pos: &[[f64; 3]]) -> Option<(usize, usize, f64)> {
    let mut best: Option<(usize, usize, f64)> = None;
    for i in 0..pos.len() {
        for j in i + 1..pos.len() {
            let d = dist(pos[i], pos[j]);
            if best.is_none_or(|b| d < b.2) {
                best = Some((i, j, d));
            }
        }
    }
    best
}

/// Result of the spacing check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpacingLevel {
    /// Spacing ≥ 1.5 × required.
    Ok,
    /// Required ≤ spacing < 1.5 × required: legal but tight.
    Tight,
    /// Spacing < required: the keep-out bubbles overlap (collision risk).
    Conflict,
}

/// Margin factor above the required spacing below which the formation is called "tight".
pub const TIGHT_FACTOR: f64 = 1.5;

/// Required centre-to-centre spacing: vehicle size + both vehicles' position error.
/// `size`: largest vehicle dimension (span or rotor-tip to rotor-tip) [m];
/// `pos_err`: horizontal position error per vehicle (e.g. GNSS 95 %) [m].
pub fn required_spacing(size: f64, pos_err: f64) -> f64 {
    size.max(0.0) + 2.0 * pos_err.max(0.0)
}

pub fn spacing_level(actual: f64, required: f64) -> SpacingLevel {
    if !(actual.is_finite() && required.is_finite()) || actual < required {
        SpacingLevel::Conflict
    } else if actual < TIGHT_FACTOR * required {
        SpacingLevel::Tight
    } else {
        SpacingLevel::Ok
    }
}

/// Every pair closer than `TIGHT_FACTOR × required`, worst first: `(i, j, d, level)`.
pub fn spacing_report(pos: &[[f64; 3]], required: f64) -> Vec<(usize, usize, f64, SpacingLevel)> {
    let mut v = Vec::new();
    for i in 0..pos.len() {
        for j in i + 1..pos.len() {
            let d = dist(pos[i], pos[j]);
            let lv = spacing_level(d, required);
            if lv != SpacingLevel::Ok {
                v.push((i, j, d, lv));
            }
        }
    }
    v.sort_by(|a, b| a.2.total_cmp(&b.2));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(kind: FormationKind, n: usize) -> FormationParams {
        FormationParams {
            kind,
            n,
            spacing: 2.0,
            step_x: 3.0,
            step_h: 0.5,
        }
    }

    #[test]
    fn every_shape_has_n_distinct_positions() {
        for kind in FormationKind::ALL {
            for n in [1, 2, 5, 9] {
                let p = positions(&fp(kind, n));
                assert_eq!(p.len(), n, "{kind:?}");
                assert!(p.iter().flatten().all(|v| v.is_finite()));
                if n > 1 {
                    let (_, _, d) = min_pair(&p).unwrap();
                    assert!(d > 1.0, "{kind:?} n={n}: {d}");
                }
            }
        }
    }

    #[test]
    fn ring_and_line_neighbour_spacing_equals_input() {
        let p = positions(&fp(FormationKind::Circle, 6));
        assert!((dist(p[0], p[1]) - 2.0).abs() < 1e-9);
        let p = positions(&fp(FormationKind::Line, 4));
        assert!((dist(p[1], p[2]) - 2.0).abs() < 1e-12);
        let p = positions(&fp(FormationKind::Vee, 3));
        assert_eq!(p[1], [3.0, -2.0, -0.5]);
        assert_eq!(p[2], [3.0, 2.0, -0.5]);
    }

    #[test]
    fn spacing_warning_switches_exactly_at_threshold() {
        let req = required_spacing(0.5, 1.5);
        assert!((req - 3.5).abs() < 1e-12);
        assert_eq!(spacing_level(3.5, req), SpacingLevel::Tight);
        assert_eq!(spacing_level(3.5 - 1e-9, req), SpacingLevel::Conflict);
        assert_eq!(spacing_level(1.5 * 3.5, req), SpacingLevel::Ok);
        assert_eq!(spacing_level(1.5 * 3.5 - 1e-9, req), SpacingLevel::Tight);
        assert_eq!(spacing_level(f64::NAN, req), SpacingLevel::Conflict);
        let pos = positions(&FormationParams {
            kind: FormationKind::Line,
            n: 3,
            spacing: 3.0,
            step_x: 0.0,
            step_h: 0.0,
        });
        let rep = spacing_report(&pos, req);
        assert_eq!(rep.len(), 2);
        assert!(rep.iter().all(|r| r.3 == SpacingLevel::Conflict));
    }
}
