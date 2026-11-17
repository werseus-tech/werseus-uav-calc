// SPDX-License-Identifier: AGPL-3.0-or-later
//! Module for calculating latency, stopping distances, and safety verdicts.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatencyIn {
    pub fps: f64,
    pub exposure_readout_ms: f64,
    pub inference_ms: f64,
    pub comm_ms: f64,
    pub fc_loop_ms: f64,
    pub speed_ms: f64,
    pub detect_range_m: f64,
    pub max_decel_ms2: f64,
}

impl Default for LatencyIn {
    fn default() -> Self {
        Self {
            fps: 30.0,
            exposure_readout_ms: 15.0,
            inference_ms: 40.0,
            comm_ms: 5.0,
            fc_loop_ms: 4.0,
            speed_ms: 10.0,
            detect_range_m: 30.0,
            max_decel_ms2: 5.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatencyOut {
    pub frame_wait_ms: f64,
    pub total_latency_ms: f64,
    pub blind_distance_m: f64,
    pub braking_distance_m: f64,
    pub stopping_distance_m: f64,
    pub margin_m: f64,
    pub max_safe_speed_ms: f64,
    pub verdict: Verdict,
}

pub fn sanitize(x: f64, lo: f64, hi: f64) -> f64 {
    if x.is_nan() { lo } else { x.clamp(lo, hi) }
}

pub fn evaluate(i: &LatencyIn) -> LatencyOut {
    let fps = sanitize(i.fps, 0.1, 10000.0);
    let exposure_readout_ms = sanitize(i.exposure_readout_ms, 0.0, 1.0e6);
    let inference_ms = sanitize(i.inference_ms, 0.0, 1.0e6);
    let comm_ms = sanitize(i.comm_ms, 0.0, 1.0e6);
    let fc_loop_ms = sanitize(i.fc_loop_ms, 0.0, 1.0e6);
    let speed_ms = sanitize(i.speed_ms, 0.0, 1.0e4);
    let detect_range_m = sanitize(i.detect_range_m, 0.0, 1.0e6);
    let max_decel_ms2 = sanitize(i.max_decel_ms2, 0.01, 1.0e4);

    let frame_wait_ms = 1000.0 / fps;
    let total_latency_ms =
        frame_wait_ms + exposure_readout_ms + inference_ms + comm_ms + fc_loop_ms;
    let t = total_latency_ms / 1000.0;

    let blind_distance_m = speed_ms * t;
    let braking_distance_m = (speed_ms * speed_ms) / (2.0 * max_decel_ms2);
    let stopping_distance_m = blind_distance_m + braking_distance_m;
    let margin_m = detect_range_m - stopping_distance_m;

    let max_safe_speed_ms = if detect_range_m == 0.0 {
        0.0
    } else {
        let r = detect_range_m;
        let a = max_decel_ms2;
        2.0 * r / (t + (t * t + 2.0 * r / a).sqrt())
    };

    let verdict = if stopping_distance_m > detect_range_m {
        Verdict::Fail
    } else if stopping_distance_m > 0.8 * detect_range_m {
        Verdict::Warn
    } else {
        Verdict::Pass
    };

    LatencyOut {
        frame_wait_ms,
        total_latency_ms,
        blind_distance_m,
        braking_distance_m,
        stopping_distance_m,
        margin_m,
        max_safe_speed_ms,
        verdict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn latency_sums() {
        let input = LatencyIn {
            fps: 50.0,
            exposure_readout_ms: 10.0,
            inference_ms: 30.0,
            comm_ms: 5.0,
            fc_loop_ms: 5.0,
            speed_ms: 0.0,
            detect_range_m: 0.0,
            max_decel_ms2: 1.0,
        };
        let output = evaluate(&input);
        assert!(approx(output.total_latency_ms, 70.0, 1e-9));
    }

    #[test]
    fn hand_values() {
        let input = LatencyIn {
            fps: 100.0,
            exposure_readout_ms: 10.0,
            inference_ms: 20.0,
            comm_ms: 5.0,
            fc_loop_ms: 5.0,
            speed_ms: 10.0,
            detect_range_m: 30.0,
            max_decel_ms2: 5.0,
        };
        let output = evaluate(&input);
        assert!(approx(output.total_latency_ms, 50.0, 1e-9));
        assert!(approx(output.blind_distance_m, 0.5, 1e-9));
        assert!(approx(output.braking_distance_m, 10.0, 1e-9));
        assert!(approx(output.stopping_distance_m, 10.5, 1e-9));
        assert!(approx(output.margin_m, 19.5, 1e-9));
        assert_eq!(output.verdict, Verdict::Pass);
    }

    #[test]
    fn verdict_levels() {
        // Warn case
        let input_warn = LatencyIn {
            fps: 100.0,
            exposure_readout_ms: 10.0,
            inference_ms: 20.0,
            comm_ms: 5.0,
            fc_loop_ms: 5.0,
            speed_ms: 15.0,
            detect_range_m: 25.0,
            max_decel_ms2: 5.0,
        };
        let output_warn = evaluate(&input_warn);
        assert!(approx(output_warn.stopping_distance_m, 23.25, 1e-9));
        assert_eq!(output_warn.verdict, Verdict::Warn);

        // Fail case
        let input_fail = LatencyIn {
            fps: 100.0,
            exposure_readout_ms: 10.0,
            inference_ms: 20.0,
            comm_ms: 5.0,
            fc_loop_ms: 5.0,
            speed_ms: 20.0,
            detect_range_m: 25.0,
            max_decel_ms2: 5.0,
        };
        let output_fail = evaluate(&input_fail);
        assert!(approx(output_fail.stopping_distance_m, 41.0, 1e-9));
        assert_eq!(output_fail.verdict, Verdict::Fail);
    }

    #[test]
    fn max_safe_speed_satisfies_equation() {
        let cases = vec![
            (100.0, 10.0, 20.0, 5.0, 5.0, 30.0, 5.0),
            (50.0, 20.0, 40.0, 10.0, 10.0, 100.0, 10.0),
            (10.0, 5.0, 5.0, 1.0, 1.0, 10.0, 1.0),
        ];

        for (fps, exp, inf, comm, fc, range, decel) in cases {
            let input = LatencyIn {
                fps,
                exposure_readout_ms: exp,
                inference_ms: inf,
                comm_ms: comm,
                fc_loop_ms: fc,
                speed_ms: 0.0,
                detect_range_m: range,
                max_decel_ms2: decel,
            };
            let output = evaluate(&input);
            let v = output.max_safe_speed_ms;
            let t = output.total_latency_ms / 1000.0;
            let calculated_stopping = v * t + (v * v) / (2.0 * decel);
            assert!(approx(calculated_stopping, range, 1e-9));

            let input_check = LatencyIn {
                speed_ms: v,
                ..input
            };
            let output_check = evaluate(&input_check);
            assert!(approx(output_check.margin_m, 0.0, 1e-9));
        }
    }

    #[test]
    fn no_nan() {
        let values = vec![
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::MAX,
            f64::NEG_INFINITY,
        ];

        for v in values {
            let input = LatencyIn {
                fps: v,
                exposure_readout_ms: v,
                inference_ms: v,
                comm_ms: v,
                fc_loop_ms: v,
                speed_ms: v,
                detect_range_m: v,
                max_decel_ms2: v,
            };
            let output = evaluate(&input);
            assert!(output.frame_wait_ms.is_finite());
            assert!(output.total_latency_ms.is_finite());
            assert!(output.blind_distance_m.is_finite());
            assert!(output.braking_distance_m.is_finite());
            assert!(output.stopping_distance_m.is_finite());
            assert!(output.margin_m.is_finite());
            assert!(output.max_safe_speed_ms.is_finite());
        }
    }

    #[test]
    fn zero_range() {
        let input = LatencyIn {
            detect_range_m: 0.0,
            ..Default::default()
        };
        let output = evaluate(&input);
        assert!(approx(output.max_safe_speed_ms, 0.0, 1e-9));
    }
}
