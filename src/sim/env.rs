// SPDX-License-Identifier: AGPL-3.0-or-later
//! Environment: steady wind with a logarithmic boundary-layer profile, first-order
//! (Dryden-like) turbulence from a seeded generator, and a discrete 1-cosine gust.
use super::math::{Rng, V3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindConfig {
    /// Mean wind speed at 10 m above ground [m/s].
    pub speed: f64,
    /// Direction the wind blows FROM, degrees clockwise from north.
    pub from_deg: f64,
    /// Turbulence intensity 0 (calm) .. 1 (strong).
    pub turbulence: f64,
}

impl Default for WindConfig {
    fn default() -> Self {
        Self {
            speed: 0.0,
            from_deg: 270.0,
            turbulence: 0.0,
        }
    }
}

const GUST_DURATION: f64 = 2.0;
const GUST_AMPLITUDE: f64 = 5.0;
const TURB_LENGTH: f64 = 50.0;
const ROUGHNESS: f64 = 0.05;

#[derive(Clone, Debug)]
pub struct Wind {
    pub cfg: WindConfig,
    rng: Rng,
    turb: V3,
    gust_t: Option<f64>,
}

impl Wind {
    pub fn new(cfg: WindConfig, seed: u64) -> Wind {
        Wind {
            cfg,
            rng: Rng::new(seed),
            turb: V3::ZERO,
            gust_t: None,
        }
    }

    pub fn trigger_gust(&mut self) {
        self.gust_t = Some(0.0);
    }

    pub fn gust_active(&self) -> bool {
        self.gust_t.is_some()
    }

    /// Advances turbulence and gust state by `dt` for an aircraft flying at `airspeed`.
    pub fn update(&mut self, dt: f64, airspeed: f64) {
        let c = &self.cfg;
        let intensity = if c.turbulence.is_finite() {
            c.turbulence.clamp(0.0, 1.0)
        } else {
            0.0
        };
        if intensity > 0.0 {
            let sigma_u = intensity * (1.0 + 0.15 * c.speed.max(0.0));
            let sig = V3::new(sigma_u, sigma_u, 0.7 * sigma_u);
            let t = TURB_LENGTH / airspeed.max(3.0);
            let a = (-dt / t).exp();
            let b = (1.0 - a * a).sqrt();
            let n = V3::new(self.rng.normal(), self.rng.normal(), self.rng.normal());
            self.turb = self.turb * a + n.mul_elem(sig) * b;
        } else {
            self.turb = V3::ZERO;
        }
        if let Some(t) = self.gust_t.as_mut() {
            *t += dt;
            if *t > GUST_DURATION {
                self.gust_t = None;
            }
        }
    }

    /// Air velocity (NED, the direction the air moves) at height `agl` above ground [m].
    pub fn velocity(&self, agl: f64) -> V3 {
        let c = &self.cfg;
        let speed = if c.speed.is_finite() {
            c.speed.clamp(0.0, 40.0)
        } else {
            0.0
        };
        let h = agl.clamp(0.3, 500.0);
        let profile = ((h / ROUGHNESS).ln() / (10.0 / ROUGHNESS).ln()).clamp(0.3, 1.5);
        let from = c.from_deg.to_radians();
        let mean = V3::new(-from.cos(), -from.sin(), 0.0) * (speed * profile);
        let gust = match self.gust_t {
            Some(t) => {
                let k = 0.5 * (1.0 - (std::f64::consts::TAU * t / GUST_DURATION).cos());
                // Crosswind gust with an updraft component.
                let side = from + std::f64::consts::FRAC_PI_2;
                V3::new(side.cos(), side.sin(), -0.3) * (GUST_AMPLITUDE * k)
            }
            None => V3::ZERO,
        };
        // Turbulence fades close to the ground.
        let fade = (h / 5.0).min(1.0);
        mean + gust + self.turb * fade
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calm_air_is_still_and_gust_ends() {
        let mut w = Wind::new(WindConfig::default(), 1);
        assert_eq!(w.velocity(10.0), V3::ZERO);
        w.trigger_gust();
        let mut peak: f64 = 0.0;
        for _ in 0..300 {
            w.update(0.01, 15.0);
            peak = peak.max(w.velocity(10.0).norm());
        }
        assert!(peak > 4.0 && !w.gust_active());
        assert_eq!(w.velocity(10.0), V3::ZERO);
    }

    #[test]
    fn wind_direction_and_profile() {
        let cfg = WindConfig {
            speed: 5.0,
            from_deg: 270.0, // from the west, blowing east
            turbulence: 0.0,
        };
        let w = Wind::new(cfg, 1);
        let v = w.velocity(10.0);
        assert!((v.y - 5.0).abs() < 1e-9 && v.x.abs() < 1e-9);
        assert!(w.velocity(1.0).norm() < v.norm());
    }

    #[test]
    fn turbulence_is_bounded() {
        let cfg = WindConfig {
            speed: 8.0,
            from_deg: 0.0,
            turbulence: 1.0,
        };
        let mut w = Wind::new(cfg, 42);
        for _ in 0..20_000 {
            w.update(0.0025, 18.0);
            let v = w.velocity(50.0);
            assert!(v.is_finite() && v.norm() < 40.0);
        }
    }
}
