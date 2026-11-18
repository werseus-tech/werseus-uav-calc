// SPDX-License-Identifier: AGPL-3.0-or-later
//! Yürüyen kareler (marching squares) ile eşik konturu: hücre merkezi yoğunluk alanından kapalı
//! çokgen çizgiler (polyline). Alan etrafına boş (0) bir sınır eklenir, bu yüzden tüm konturlar
//! kapalıdır. Çıktı sırası belirlenimcidir.

use std::collections::BTreeMap;

/// Kapalı çokgen çizgi (ilk nokta sona tekrarlanmaz).
pub type Loop = Vec<[f64; 2]>;

/// `field` (nx × ny, satır ana sıralı, y yukarı) için `iso` seviyesinde konturlar. Koordinatlar
/// hücre birimindedir: hücre (i, j) merkezi (i + 0.5, j + 0.5).
pub fn marching_squares(field: &[f64], nx: usize, ny: usize, iso: f64) -> Vec<Loop> {
    assert_eq!(field.len(), nx * ny);
    // Boş sınırla genişletilmiş örnek ızgarası: (W × H), örnek (a, b) = hücre (a-1, b-1).
    let (w, h) = (nx + 2, ny + 2);
    let val = |a: usize, b: usize| -> f64 {
        if a == 0 || b == 0 || a > nx || b > ny {
            0.0
        } else {
            field[(b - 1) * nx + (a - 1)]
        }
    };
    let pos = |a: usize, b: usize| -> [f64; 2] { [a as f64 - 0.5, b as f64 - 0.5] };
    // Kenar kimliği: yatay (a,b)-(a+1,b) -> 2*(b*w+a); dikey (a,b)-(a,b+1) -> 2*(b*w+a)+1.
    let hid = |a: usize, b: usize| 2 * (b * w + a);
    let vid = |a: usize, b: usize| 2 * (b * w + a) + 1;
    let mut pts: BTreeMap<usize, [f64; 2]> = BTreeMap::new();
    let mut segs: Vec<(usize, usize)> = Vec::new();
    let lerp = |p0: [f64; 2], p1: [f64; 2], v0: f64, v1: f64| -> [f64; 2] {
        let t = if (v1 - v0).abs() < 1e-12 {
            0.5
        } else {
            ((iso - v0) / (v1 - v0)).clamp(0.0, 1.0)
        };
        [p0[0] + t * (p1[0] - p0[0]), p0[1] + t * (p1[1] - p0[1])]
    };
    for b in 0..h - 1 {
        for a in 0..w - 1 {
            let v = [val(a, b), val(a + 1, b), val(a + 1, b + 1), val(a, b + 1)];
            let inside = [v[0] > iso, v[1] > iso, v[2] > iso, v[3] > iso];
            let code = inside
                .iter()
                .enumerate()
                .fold(0u8, |c, (k, &i)| c | ((i as u8) << k));
            if code == 0 || code == 15 {
                continue;
            }
            // Kenarlar: 0 alt (0-1), 1 sağ (1-2), 2 üst (3-2), 3 sol (0-3).
            let eid = [hid(a, b), vid(a + 1, b), hid(a, b + 1), vid(a, b)];
            let p = [pos(a, b), pos(a + 1, b), pos(a + 1, b + 1), pos(a, b + 1)];
            let cross = |e: usize, pts: &mut BTreeMap<usize, [f64; 2]>| {
                let (i0, i1) = match e {
                    0 => (0, 1),
                    1 => (1, 2),
                    2 => (3, 2),
                    _ => (0, 3),
                };
                pts.entry(eid[e])
                    .or_insert_with(|| lerp(p[i0], p[i1], v[i0], v[i1]));
                eid[e]
            };
            let mut add = |e0: usize, e1: usize, pts: &mut BTreeMap<usize, [f64; 2]>| {
                let (i0, i1) = (cross(e0, pts), cross(e1, pts));
                segs.push((i0, i1));
            };
            match code {
                1 | 14 => add(3, 0, &mut pts),
                2 | 13 => add(0, 1, &mut pts),
                3 | 12 => add(3, 1, &mut pts),
                4 | 11 => add(1, 2, &mut pts),
                6 | 9 => add(0, 2, &mut pts),
                7 | 8 => add(3, 2, &mut pts),
                5 | 10 => {
                    // Eyer: merkez ortalamasına göre ayır.
                    let centre = 0.25 * (v[0] + v[1] + v[2] + v[3]) > iso;
                    let joined = (code == 5) == centre;
                    if joined {
                        add(3, 2, &mut pts);
                        add(0, 1, &mut pts);
                    } else {
                        add(3, 0, &mut pts);
                        add(1, 2, &mut pts);
                    }
                }
                _ => {}
            }
        }
    }
    // Uç eşlemesi: her kontur noktası iki doğru parçasına aittir.
    let mut adj: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (k, (a, b)) in segs.iter().enumerate() {
        adj.entry(*a).or_default().push(k);
        adj.entry(*b).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut loops = Vec::new();
    for k0 in 0..segs.len() {
        if used[k0] {
            continue;
        }
        used[k0] = true;
        let (start, mut cur) = segs[k0];
        let mut ids = vec![start, cur];
        loop {
            if cur == start {
                ids.pop();
                break;
            }
            let next = adj[&cur].iter().copied().find(|&s| !used[s]);
            let Some(s) = next else { break };
            used[s] = true;
            let (a, b) = segs[s];
            cur = if a == cur { b } else { a };
            ids.push(cur);
        }
        if ids.len() >= 3 {
            loops.push(ids.iter().map(|i| pts[i]).collect());
        }
    }
    loops
}

/// Douglas-Peucker ile kapalı çokgen sadeleştirme (`tol` hücre birimi).
pub fn simplify_closed(lp: &Loop, tol: f64) -> Loop {
    let n = lp.len();
    if n < 4 || tol <= 0.0 {
        return lp.clone();
    }
    // Birbirinden en uzak iki noktadan böl (kapalı eğri için kararlı).
    let i0 = 0;
    let mut i1 = 0;
    let mut best = -1.0;
    for (k, p) in lp.iter().enumerate() {
        let d = (p[0] - lp[i0][0]).powi(2) + (p[1] - lp[i0][1]).powi(2);
        if d > best {
            best = d;
            i1 = k;
        }
    }
    let mut keep = vec![false; n];
    keep[i0] = true;
    keep[i1] = true;
    let mut stack = vec![(i0, i1), (i1, i0 + n)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (lp[a % n], lp[b % n]);
        let (mut dmax, mut imax) = (-1.0, a);
        for k in a + 1..b {
            let d = point_seg_dist(lp[k % n], pa, pb);
            if d > dmax {
                dmax = d;
                imax = k;
            }
        }
        if dmax > tol {
            keep[imax % n] = true;
            stack.push((a, imax));
            stack.push((imax, b));
        }
    }
    let out: Loop = lp
        .iter()
        .enumerate()
        .filter(|(k, _)| keep[*k])
        .map(|(_, p)| *p)
        .collect();
    if out.len() >= 3 { out } else { lp.clone() }
}

fn point_seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-18 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    };
    let (qx, qy) = (a[0] + t * dx, a[1] + t * dy);
    ((p[0] - qx).powi(2) + (p[1] - qy).powi(2)).sqrt()
}

/// İşaretli alan (shoelace); çokgenin mutlak alanı için `.abs()`.
pub fn signed_area(lp: &Loop) -> f64 {
    let n = lp.len();
    let mut s = 0.0;
    for i in 0..n {
        let (a, b) = (lp[i], lp[(i + 1) % n]);
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_block_gives_one_closed_loop() {
        // 6x6 alanda ortada 2x2 dolu blok.
        let (nx, ny) = (6, 6);
        let mut f = vec![0.0; nx * ny];
        for j in 2..4 {
            for i in 2..4 {
                f[j * nx + i] = 1.0;
            }
        }
        let l = marching_squares(&f, nx, ny, 0.5);
        assert_eq!(l.len(), 1);
        // Merkezler 2.5..3.5; kontur kenarlarda yarı yolda (2..4), köşeler kesik: 4 - 4*0.125.
        assert!((signed_area(&l[0]).abs() - 3.5).abs() < 1e-9);
    }

    #[test]
    fn ring_gives_outer_and_hole_and_saddle_is_closed() {
        let (nx, ny) = (10, 10);
        let mut f = vec![1.0; nx * ny];
        for j in 4..6 {
            for i in 4..6 {
                f[j * nx + i] = 0.0;
            }
        }
        let l = marching_squares(&f, nx, ny, 0.5);
        assert_eq!(l.len(), 2);
        // Eyer: çapraz iki dolu hücre.
        let g = vec![1.0, 0.0, 0.0, 1.0];
        let l = marching_squares(&g, 2, 2, 0.5);
        assert!(!l.is_empty());
        for lp in &l {
            assert!(lp.len() >= 3);
        }
    }

    #[test]
    fn simplify_keeps_shape() {
        let (nx, ny) = (20, 20);
        let mut f = vec![0.0; nx * ny];
        for j in 0..ny {
            for i in 0..nx {
                let (x, y) = (i as f64 - 9.5, j as f64 - 9.5);
                f[j * nx + i] = if x * x + y * y < 49.0 { 1.0 } else { 0.0 };
            }
        }
        let l = marching_squares(&f, nx, ny, 0.5);
        assert_eq!(l.len(), 1);
        let s = simplify_closed(&l[0], 0.2);
        assert!(s.len() < l[0].len());
        assert!((signed_area(&s).abs() - signed_area(&l[0]).abs()).abs() < 0.1 * 150.0);
    }
}
