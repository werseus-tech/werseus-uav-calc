// SPDX-License-Identifier: AGPL-3.0-or-later
//! Zemin ızgarası yardımcıları (metre, Z yukarı).

/// One grid line segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLine {
    pub a: [f32; 3],
    pub b: [f32; 3],
    /// Every 5th line (world index divisible by 5) is a major line; the lines through the world
    /// origin (index 0) are also major.
    pub major: bool,
}

fn is_finite(f: f32) -> bool {
    f.is_finite()
}

/// A "nice" grid spacing of the form {1, 2, 5} * 10^k such that extent / step is closest to
/// `target_lines` (ties -> the larger step). Invalid inputs -> 1.0.
pub fn nice_step(extent: f32, target_lines: u32) -> f32 {
    if target_lines == 0 || !is_finite(extent) || extent <= 0.0 {
        return 1.0;
    }
    let target = target_lines as f64;
    let ext = extent as f64;
    let raw = ext / target;
    let k0 = raw.log10().floor() as i32;
    let mut best = (f64::INFINITY, 1.0f64);
    for k in (k0 - 1)..=(k0 + 1) {
        for mult in [1.0f64, 2.0, 5.0] {
            let step = mult * 10f64.powi(k);
            let err = (ext / step - target).abs();
            if err < best.0 - 1e-9 || ((err - best.0).abs() <= 1e-9 && step > best.1) {
                best = (err, step);
            }
        }
    }
    if best.1.is_finite() && best.1 > 0.0 {
        best.1 as f32
    } else {
        1.0
    }
}

/// Grid lines covering the square [cx-half, cx+half] x [cy-half, cy+half] at height z0.
/// World-aligned integer multiples of `step`. Returns empty Vec on invalid input or >400 lines per axis.
pub fn grid_lines(cx: f32, cy: f32, half: f32, step: f32, z0: f32) -> Vec<GridLine> {
    if !is_finite(cx)
        || !is_finite(cy)
        || !is_finite(half)
        || !is_finite(step)
        || !is_finite(z0)
        || step <= 0.0
        || half <= 0.0
    {
        return Vec::new();
    }

    let ratio = 2.0 * half / step;
    if ratio > 400.0 {
        return Vec::new();
    }

    let xmin = cx - half;
    let xmax = cx + half;
    let ymin = cy - half;
    let ymax = cy + half;

    let i_min = (xmin / step).ceil() as i64;
    let i_max = (xmax / step).floor() as i64;

    if i_min > i_max {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Y-parallel lines (varying x at i*step)
    for i in i_min..=i_max {
        let x = (i as f64 * step as f64) as f32;
        let major = i.rem_euclid(5) == 0;
        out.push(GridLine {
            a: [x, ymin, z0],
            b: [x, ymax, z0],
            major,
        });
    }

    // X-parallel lines (varying y at j*step)
    let j_min = (ymin / step).ceil() as i64;
    let j_max = (ymax / step).floor() as i64;

    for j in j_min..=j_max {
        let y = (j as f64 * step as f64) as f32;
        let major = j.rem_euclid(5) == 0;
        out.push(GridLine {
            a: [xmin, y, z0],
            b: [xmax, y, z0],
            major,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_step_example1() {
        assert_eq!(nice_step(2.0, 20), 0.1);
    }

    #[test]
    fn nice_step_example2() {
        assert_eq!(nice_step(10.0, 10), 1.0);
    }

    #[test]
    fn nice_step_example3() {
        // 0.3/0.02=15 -> |15-10|=5, 0.3/0.05=6 -> |6-10|=4, so 0.05
        assert_eq!(nice_step(0.3, 10), 0.05);
    }

    #[test]
    fn nice_step_zero_extent() {
        assert_eq!(nice_step(0.0, 10), 1.0);
    }

    #[test]
    fn nice_step_nan() {
        assert_eq!(nice_step(f32::NAN, 10), 1.0);
    }

    #[test]
    fn grid_basic_counts_and_major() {
        let lines = grid_lines(0.0, 0.0, 1.0, 0.5, 0.0);
        // range -1..=1 step 0.5 -> i in -2,-1,0,1,2 -> 5 per axis -> 10 total
        assert_eq!(lines.len(), 10);

        // Y-parallel lines: a[0]=b[0]
        let y_parallel: Vec<&GridLine> = lines
            .iter()
            .filter(|l| (l.a[0] - l.b[0]).abs() < 1e-7)
            .collect();
        assert_eq!(y_parallel.len(), 5);

        // X-parallel lines: a[1]=b[1]
        let x_parallel: Vec<&GridLine> = lines
            .iter()
            .filter(|l| (l.a[1] - l.b[1]).abs() < 1e-7)
            .collect();
        assert_eq!(x_parallel.len(), 5);

        // Lines at x=0 (i=0) should be major
        let at_zero_x: Vec<&GridLine> = y_parallel
            .iter()
            .copied()
            .filter(|l| l.a[0].abs() < 1e-7)
            .collect();
        assert_eq!(at_zero_x.len(), 1);
        assert!(at_zero_x[0].major);

        // Lines at y=0 (j=0) should be major
        let at_zero_y: Vec<&GridLine> = x_parallel
            .iter()
            .copied()
            .filter(|l| l.a[1].abs() < 1e-7)
            .collect();
        assert_eq!(at_zero_y.len(), 1);
        assert!(at_zero_y[0].major);

        // i=-5 not in range but i=-2, i=2 should NOT be major (2%5=2, -2%5=3)
        for l in &y_parallel {
            if l.a[0].abs() > 1e-7 {
                assert!(!l.major);
            }
        }
        for l in &x_parallel {
            if l.a[1].abs() > 1e-7 {
                assert!(!l.major);
            }
        }
    }

    #[test]
    fn grid_z_height() {
        let lines = grid_lines(0.0, 0.0, 1.0, 0.5, 42.0);
        assert!(!lines.is_empty());
        for l in &lines {
            assert!((l.a[2] - 42.0).abs() < 1e-7);
            assert!((l.b[2] - 42.0).abs() < 1e-7);
        }
    }

    #[test]
    fn grid_world_aligned() {
        // Shift centre; Y-parallel lines should still sit at multiples of 0.5
        let lines = grid_lines(0.26, 0.0, 1.0, 0.5, 0.0);
        assert!(!lines.is_empty());

        let y_parallel: Vec<&GridLine> = lines
            .iter()
            .filter(|l| (l.a[0] - l.b[0]).abs() < 1e-7)
            .collect();

        for l in &y_parallel {
            let x = l.a[0];
            let k = (x / 0.5).round();
            assert!((x - k * 0.5).abs() < 1e-5, "x={} not a multiple of 0.5", x);
        }
    }

    #[test]
    fn grid_too_many_lines_empty() {
        // 2*half/step > 400 -> empty
        // half=1000, step=1 -> ratio=2000 > 400
        let lines = grid_lines(0.0, 0.0, 1000.0, 1.0, 0.0);
        assert!(lines.is_empty());
    }

    #[test]
    fn grid_nan_input_empty() {
        assert!(grid_lines(f32::NAN, 0.0, 1.0, 0.5, 0.0).is_empty());
        assert!(grid_lines(0.0, 0.0, 1.0, 0.5, f32::NAN).is_empty());
        assert!(grid_lines(0.0, 0.0, f32::NAN, 0.5, 0.0).is_empty());
    }

    #[test]
    fn negative_index_five_is_major() {
        // Choose extent so that i=-5 is included: range [-5*step, +5*step]
        // step=1.0, half=5.0 -> i in -5..=5
        let lines = grid_lines(0.0, 0.0, 5.0, 1.0, 0.0);
        // i=-5: x=-5, should be major
        let at_neg5: Vec<&GridLine> = lines
            .iter()
            .filter(|l| (l.a[0] - l.b[0]).abs() < 1e-7 && (l.a[0] + 5.0).abs() < 1e-7)
            .collect();
        assert_eq!(at_neg5.len(), 1);
        assert!(at_neg5[0].major, "i=-5 should be major");
    }
}
