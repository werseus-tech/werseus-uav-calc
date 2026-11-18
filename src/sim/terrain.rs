// SPDX-License-Identifier: AGPL-3.0-or-later
//! Procedural flight-simulator environments: OpenField, Runway, City, Mountains.
//!
//! Pure geometry/heightmap queries (no egui/GL dependency) built deterministically from a
//! fixed seed. Coordinates in physics queries are NED `(x = North, y = East)` with
//! `ground_height(x, y)` returning height UP above the `z = 0` datum [m].
//! [`WorldMesh`] outputs render-world coordinates (`x = North, y = West (-East), z = Up`).

use super::math::V3;

/// Fixed deterministic seed for all procedural environments.
pub const ENV_SEED: u64 = 0x5745_5253_4555_5331;

/// Hard triangle budget per generated world mesh so rendering stays fast.
pub const WORLD_TRIANGLE_CAP: usize = 12_000;

const CITY_PITCH: f64 = 60.0;
const STREET_HALF: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EnvironmentKind {
    OpenField,
    #[default]
    Runway,
    City,
    Mountains,
}

impl EnvironmentKind {
    pub const ALL: [EnvironmentKind; 4] = [
        EnvironmentKind::OpenField,
        EnvironmentKind::Runway,
        EnvironmentKind::City,
        EnvironmentKind::Mountains,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceType {
    Grass,
    Asphalt,
    Concrete,
    Rock,
    Snow,
}

/// Axis-aligned building footprint in NED `(x = North, y = East)` with height [m] above `z=0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingBox {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
    pub height: f64,
    pub seed: u64,
}

/// Vertex-coloured triangle mesh in render-world coordinates (`x = North, y = West, z = Up`).
#[derive(Clone, Debug, Default)]
pub struct WorldMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub version: u64,
}

impl WorldMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn is_valid(&self) -> bool {
        let n = self.positions.len();
        self.normals.len() == n
            && self.colors.len() == n
            && self.indices.len().is_multiple_of(3)
            && self
                .positions
                .iter()
                .all(|p| p[0].is_finite() && p[1].is_finite() && p[2].is_finite())
            && self
                .normals
                .iter()
                .all(|q| q[0].is_finite() && q[1].is_finite() && q[2].is_finite())
            && self
                .colors
                .iter()
                .all(|c| c.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
            && self.indices.iter().all(|&i| (i as usize) < n)
    }

    fn can_add_tris(&self, n_tris: usize) -> bool {
        self.triangle_count() + n_tris <= WORLD_TRIANGLE_CAP
    }

    fn push_vertex(&mut self, p: [f32; 3], n: [f32; 3], c: [f32; 4]) -> u32 {
        let idx = self.positions.len() as u32;
        self.positions.push(p);
        self.normals.push(n);
        self.colors.push(c);
        idx
    }

    fn push_quad(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], col: [f32; 4]) {
        if !self.can_add_tris(2) {
            return;
        }
        let n = tri_normal(a, b, c);
        let i0 = self.push_vertex(a, n, col);
        let i1 = self.push_vertex(b, n, col);
        let i2 = self.push_vertex(c, n, col);
        let i3 = self.push_vertex(d, n, col);
        self.indices.extend_from_slice(&[i0, i1, i2, i0, i2, i3]);
    }

    /// Pushes a flat horizontal quad in NED `(x0..x1, y0..y1)` at height `z_up` [m].
    fn push_ned_rect(&mut self, x0: f64, x1: f64, y0: f64, y1: f64, z_up: f32, col: [f32; 4]) {
        let (x0, x1) = (x0 as f32, x1 as f32);
        let (ry0, ry1) = (-y1 as f32, -y0 as f32); // render y = -ned_y
        self.push_quad(
            [x0, ry0, z_up],
            [x1, ry0, z_up],
            [x1, ry1, z_up],
            [x0, ry1, z_up],
            col,
        );
    }

    fn push_box_top_sides(
        &mut self,
        min: [f32; 3],
        max: [f32; 3],
        wall_col: [f32; 4],
        roof_col: [f32; 4],
    ) {
        if !self.can_add_tris(10) {
            return;
        }
        let [x0, ry0, z0] = min;
        let [x1, ry1, z1] = max;
        // Roof (+Z)
        self.push_quad(
            [x0, ry0, z1],
            [x1, ry0, z1],
            [x1, ry1, z1],
            [x0, ry1, z1],
            roof_col,
        );
        // +X
        self.push_quad(
            [x1, ry0, z0],
            [x1, ry1, z0],
            [x1, ry1, z1],
            [x1, ry0, z1],
            wall_col,
        );
        // -X
        self.push_quad(
            [x0, ry1, z0],
            [x0, ry0, z0],
            [x0, ry0, z1],
            [x0, ry1, z1],
            wall_col,
        );
        // +Y
        self.push_quad(
            [x1, ry1, z0],
            [x0, ry1, z0],
            [x0, ry1, z1],
            [x1, ry1, z1],
            wall_col,
        );
        // -Y
        self.push_quad(
            [x0, ry0, z0],
            [x1, ry0, z0],
            [x1, ry0, z1],
            [x0, ry0, z1],
            wall_col,
        );
    }
}

fn tri_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ux = b[0] - a[0];
    let uy = b[1] - a[1];
    let uz = b[2] - a[2];
    let vx = c[0] - a[0];
    let vy = c[1] - a[1];
    let vz = c[2] - a[2];
    let nx = uy * vz - uz * vy;
    let ny = uz * vx - ux * vz;
    let nz = ux * vy - uy * vx;
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len > 1e-9 && len.is_finite() {
        [nx / len, ny / len, nz / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

// ---------------------------------------------------------------------------------------------
// Deterministic hashing & 2D smooth value noise
// ---------------------------------------------------------------------------------------------

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn hash2(seed: u64, ix: i64, iy: i64) -> u64 {
    let h = seed ^ (ix as u64).wrapping_mul(0x517C_C1B7_2722_0A95);
    splitmix64(h ^ (iy as u64).wrapping_mul(0x94D0_49BB_1331_11EB))
}

/// Deterministic float in [0, 1).
fn rand01(h: u64) -> f64 {
    ((h >> 11) as f64) / ((1u64 << 53) as f64)
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn value_noise2(seed: u64, x: f64, y: f64) -> f64 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let fx = fade(x - ix as f64);
    let fy = fade(y - iy as f64);
    let v00 = rand01(hash2(seed, ix, iy));
    let v10 = rand01(hash2(seed, ix + 1, iy));
    let v01 = rand01(hash2(seed, ix, iy + 1));
    let v11 = rand01(hash2(seed, ix + 1, iy + 1));
    let a = lerp(v00, v10, fx);
    let b = lerp(v01, v11, fx);
    lerp(a, b, fy) * 2.0 - 1.0
}

fn fbm2(seed: u64, x: f64, y: f64, octaves: usize) -> f64 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for k in 0..octaves {
        sum += amp * value_noise2(seed.wrapping_add(k as u64 * 1013), x * freq, y * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    if norm > 0.0 { sum / norm } else { 0.0 }
}

// ---------------------------------------------------------------------------------------------
// Terrain height, surface, and city building queries
// ---------------------------------------------------------------------------------------------

fn in_runway_asphalt(x: f64, y: f64) -> bool {
    ((-20.0..=240.0).contains(&x) && y.abs() <= 7.5)
        || ((-20.0..=15.0).contains(&x) && (7.5..=22.0).contains(&y))
}

fn in_city_plaza(x: f64, y: f64) -> bool {
    (-50.0..=220.0).contains(&x) && y.abs() <= 28.0
}

fn in_city_runway(x: f64, y: f64) -> bool {
    (-20.0..=210.0).contains(&x) && y.abs() <= 7.0
}

impl EnvironmentKind {
    /// Safe open spawn position `(ned_x, ned_y, height_up)` [m] for this environment.
    pub fn spawn_position(self) -> (f64, f64, f64) {
        (0.0, 0.0, self.ground_height(0.0, 0.0))
    }

    /// Ground height UP above `z = 0` datum [m] at NED `(x, y)`. Always finite.
    pub fn ground_height(self, x: f64, y: f64) -> f64 {
        if !(x.is_finite() && y.is_finite()) {
            return 0.0;
        }
        match self {
            EnvironmentKind::OpenField | EnvironmentKind::Runway | EnvironmentKind::City => 0.0,
            EnvironmentKind::Mountains => {
                let r = x.hypot(y);
                // Safe flat launch/landing valley around the origin (0, 0).
                let blend = smoothstep(45.0, 145.0, r);
                if blend <= 0.0 {
                    return 0.0;
                }
                let base = 0.5 + 0.5 * fbm2(ENV_SEED ^ 0x1111, x * 0.0022, y * 0.0022, 4);
                let ridge_raw = fbm2(ENV_SEED ^ 0x2222, x * 0.0042 + 3.1, y * 0.0042 - 1.7, 3);
                let ridge = (1.0 - ridge_raw.abs()).powf(1.55);
                let detail = 0.5 + 0.5 * fbm2(ENV_SEED ^ 0x3333, x * 0.015, y * 0.015, 2);
                let raw = 140.0 * (0.42 * base + 0.58 * ridge) + 12.0 * detail;
                (blend * raw).max(0.0)
            }
        }
    }

    /// Upward terrain normal in NED coordinates (points `-Z` on flat ground).
    pub fn ground_normal_ned(self, x: f64, y: f64) -> V3 {
        if self != EnvironmentKind::Mountains || !(x.is_finite() && y.is_finite()) {
            return V3::new(0.0, 0.0, -1.0);
        }
        let eps = 1.0;
        let hx = (self.ground_height(x + eps, y) - self.ground_height(x - eps, y)) / (2.0 * eps);
        let hy = (self.ground_height(x, y + eps) - self.ground_height(x, y - eps)) / (2.0 * eps);
        V3::new(-hx, -hy, -1.0).normalized_or(V3::new(0.0, 0.0, -1.0))
    }

    /// Surface material at NED `(x, y)`.
    pub fn surface_at(self, x: f64, y: f64) -> SurfaceType {
        if !(x.is_finite() && y.is_finite()) {
            return SurfaceType::Grass;
        }
        match self {
            EnvironmentKind::OpenField => SurfaceType::Grass,
            EnvironmentKind::Runway => {
                if in_runway_asphalt(x, y) {
                    SurfaceType::Asphalt
                } else {
                    SurfaceType::Grass
                }
            }
            EnvironmentKind::City => {
                if in_city_plaza(x, y) {
                    if in_city_runway(x, y) {
                        SurfaceType::Asphalt
                    } else {
                        SurfaceType::Grass
                    }
                } else {
                    let rx = x.rem_euclid(CITY_PITCH);
                    let ry = y.rem_euclid(CITY_PITCH);
                    let on_x_street = rx < STREET_HALF || rx > CITY_PITCH - STREET_HALF;
                    let on_y_street = ry < STREET_HALF || ry > CITY_PITCH - STREET_HALF;
                    if on_x_street || on_y_street {
                        SurfaceType::Asphalt
                    } else {
                        SurfaceType::Concrete
                    }
                }
            }
            EnvironmentKind::Mountains => {
                let h = self.ground_height(x, y);
                if h < 1.0 && x.hypot(y) < 40.0 {
                    return SurfaceType::Asphalt;
                }
                let n_up_z = -self.ground_normal_ned(x, y).z;
                let slope = (1.0 - n_up_z).clamp(0.0, 1.0);
                if h > 72.0 && slope < 0.36 {
                    SurfaceType::Snow
                } else if h > 34.0 || slope > 0.22 {
                    SurfaceType::Rock
                } else {
                    SurfaceType::Grass
                }
            }
        }
    }

    /// Wheel rolling resistance coefficient at NED `(x, y)`.
    pub fn rolling_mu(self, x: f64, y: f64) -> f64 {
        match self.surface_at(x, y) {
            SurfaceType::Asphalt | SurfaceType::Concrete => 0.035,
            SurfaceType::Grass => 0.085,
            SurfaceType::Rock | SurfaceType::Snow => 0.11,
        }
    }

    /// Deterministic building(s) inside city block `(bx, by)`, if any.
    pub fn city_block_buildings(bx: i32, by: i32) -> [Option<BuildingBox>; 2] {
        let block_min_x = bx as f64 * CITY_PITCH + STREET_HALF;
        let block_max_x = (bx + 1) as f64 * CITY_PITCH - STREET_HALF;
        let block_min_y = by as f64 * CITY_PITCH + STREET_HALF;
        let block_max_y = (by + 1) as f64 * CITY_PITCH - STREET_HALF;

        // Keep spawn and takeoff corridor completely open.
        if block_max_x >= -55.0
            && block_min_x <= 225.0
            && block_max_y >= -32.0
            && block_min_y <= 32.0
        {
            return [None, None];
        }
        let cx = 0.5 * (block_min_x + block_max_x);
        let cy = 0.5 * (block_min_y + block_max_y);
        if cx.hypot(cy) < 65.0 {
            return [None, None];
        }

        let h0 = hash2(ENV_SEED ^ 0xC171, bx as i64, by as i64);
        let h1 = splitmix64(h0 ^ 0x1234_5678);
        let h2 = splitmix64(h0 ^ 0x9ABC_DEF0);
        // Leave occasional open pocket parks (~12 % of blocks).
        if rand01(h0) < 0.12 {
            return [None, None];
        }
        let pad = 2.5;
        let ix0 = block_min_x + pad;
        let ix1 = block_max_x - pad;
        let iy0 = block_min_y + pad;
        let iy1 = block_max_y - pad;

        let split = rand01(h1) > 0.45;
        if split {
            let gap = 3.0;
            let mid_x = 0.5 * (ix0 + ix1) + (rand01(h2) - 0.5) * 4.0;
            let height_a = 14.0 + rand01(splitmix64(h1)) * 46.0;
            let height_b = 14.0 + rand01(splitmix64(h2)) * 56.0;
            [
                Some(BuildingBox {
                    min_x: ix0,
                    max_x: mid_x - gap * 0.5,
                    min_y: iy0 + rand01(h0 ^ 7) * 3.0,
                    max_y: iy1 - rand01(h0 ^ 13) * 3.0,
                    height: height_a,
                    seed: h1,
                }),
                Some(BuildingBox {
                    min_x: mid_x + gap * 0.5,
                    max_x: ix1,
                    min_y: iy0 + rand01(h0 ^ 19) * 3.0,
                    max_y: iy1 - rand01(h0 ^ 29) * 3.0,
                    height: height_b,
                    seed: h2,
                }),
            ]
        } else {
            let inset_x = 2.0 + rand01(h1) * 5.0;
            let inset_y = 2.0 + rand01(h2) * 5.0;
            let tall_boost = if rand01(h0 ^ 0x55) > 0.85 { 24.0 } else { 0.0 };
            let height = 16.0 + rand01(splitmix64(h0)) * 48.0 + tall_boost;
            [
                Some(BuildingBox {
                    min_x: ix0 + inset_x,
                    max_x: ix1 - inset_x,
                    min_y: iy0 + inset_y,
                    max_y: iy1 - inset_y,
                    height,
                    seed: h0,
                }),
                None,
            ]
        }
    }

    /// Returns all buildings within `radius` [m] of NED `(x, y)` (empty unless `City`).
    pub fn buildings_near(self, x: f64, y: f64, radius: f64) -> Vec<BuildingBox> {
        if self != EnvironmentKind::City || !(x.is_finite() && y.is_finite() && radius > 0.0) {
            return Vec::new();
        }
        let bx0 = ((x - radius) / CITY_PITCH).floor() as i32;
        let bx1 = ((x + radius) / CITY_PITCH).ceil() as i32;
        let by0 = ((y - radius) / CITY_PITCH).floor() as i32;
        let by1 = ((y + radius) / CITY_PITCH).ceil() as i32;
        let r2 = radius * radius;
        let mut out = Vec::new();
        for bx in bx0..=bx1 {
            for by in by0..=by1 {
                for b in Self::city_block_buildings(bx, by).into_iter().flatten() {
                    let cx = 0.5 * (b.min_x + b.max_x);
                    let cy = 0.5 * (b.min_y + b.max_y);
                    let dx = cx - x;
                    let dy = cy - y;
                    if dx * dx + dy * dy <= r2 {
                        out.push(b);
                    }
                }
            }
        }
        out
    }

    /// Checks whether NED point `(x, y)` at altitude `alt_up` [m] hits any city building.
    pub fn building_hit(self, x: f64, y: f64, alt_up: f64) -> Option<BuildingBox> {
        if self != EnvironmentKind::City || !(x.is_finite() && y.is_finite() && alt_up.is_finite())
        {
            return None;
        }
        let bx = (x / CITY_PITCH).floor() as i32;
        let by = (y / CITY_PITCH).floor() as i32;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for b in Self::city_block_buildings(bx + dx, by + dy)
                    .into_iter()
                    .flatten()
                {
                    if x >= b.min_x
                        && x <= b.max_x
                        && y >= b.min_y
                        && y <= b.max_y
                        && alt_up <= b.height
                    {
                        return Some(b);
                    }
                }
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Procedural WorldMesh generation
// ---------------------------------------------------------------------------------------------

fn mix_col(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

fn scale_col(c: [f32; 4], k: f32) -> [f32; 4] {
    [
        (c[0] * k).clamp(0.0, 1.0),
        (c[1] * k).clamp(0.0, 1.0),
        (c[2] * k).clamp(0.0, 1.0),
        c[3],
    ]
}

impl WorldMesh {
    /// Generates a deterministic procedural mesh for `env` around NED `(cx, cy)`.
    pub fn build(env: EnvironmentKind, cx: f64, cy: f64, dark: bool, version: u64) -> WorldMesh {
        let mut m = WorldMesh {
            version,
            ..Default::default()
        };
        let shade = if dark { 0.80_f32 } else { 1.0 };
        match env {
            EnvironmentKind::OpenField => build_open_field(&mut m, cx, cy, shade),
            EnvironmentKind::Runway => build_runway(&mut m, cx, cy, shade),
            EnvironmentKind::City => build_city(&mut m, cx, cy, shade),
            EnvironmentKind::Mountains => build_mountains(&mut m, cx, cy, shade),
        }
        m
    }
}

fn build_grass_grid(m: &mut WorldMesh, cx: f64, cy: f64, cells: i32, step: f64, shade: f32) {
    let qx = (cx / step).round() * step;
    let qy = (cy / step).round() * step;
    let half = cells / 2;
    let c_a = scale_col([0.23, 0.43, 0.19, 1.0], shade);
    let c_b = scale_col([0.28, 0.49, 0.22, 1.0], shade);
    for ix in -half..half {
        for iy in -half..half {
            let x0 = qx + ix as f64 * step;
            let x1 = x0 + step;
            let y0 = qy + iy as f64 * step;
            let y1 = y0 + step;
            let n = 0.5
                + 0.5
                    * value_noise2(
                        ENV_SEED ^ 0x77,
                        (x0 + step * 0.5) * 0.018,
                        (y0 + step * 0.5) * 0.018,
                    );
            let col = mix_col(c_a, c_b, n as f32);
            m.push_ned_rect(x0, x1, y0, y1, 0.0, col);
        }
    }
}

fn push_tree(m: &mut WorldMesh, x_ned: f64, y_ned: f64, h: f32, shade: f32, seed: u64) {
    if !m.can_add_tris(16) {
        return;
    }
    let rx = x_ned as f32;
    let ry = -y_ned as f32;
    let trunk_w = 0.35;
    let trunk_h = h * 0.32;
    let trunk_col = scale_col([0.32, 0.22, 0.14, 1.0], shade);
    m.push_box_top_sides(
        [rx - trunk_w, ry - trunk_w, 0.0],
        [rx + trunk_w, ry + trunk_w, trunk_h],
        trunk_col,
        trunk_col,
    );
    let r = h * (0.36 + 0.12 * rand01(seed) as f32);
    let leaf_base = scale_col([0.16, 0.36, 0.15, 1.0], shade);
    let leaf_tip = scale_col([0.22, 0.44, 0.19, 1.0], shade);
    let apex = [rx, ry, h];
    let p0 = [rx - r, ry - r, trunk_h];
    let p1 = [rx + r, ry - r, trunk_h];
    let p2 = [rx + r, ry + r, trunk_h];
    let p3 = [rx - r, ry + r, trunk_h];
    for (a, b) in [(p0, p1), (p1, p2), (p2, p3), (p3, p0)] {
        if !m.can_add_tris(1) {
            return;
        }
        let n = tri_normal(a, b, apex);
        let i0 = m.push_vertex(a, n, leaf_base);
        let i1 = m.push_vertex(b, n, leaf_base);
        let i2 = m.push_vertex(apex, n, leaf_tip);
        m.indices.extend_from_slice(&[i0, i1, i2]);
    }
}

fn build_open_field(m: &mut WorldMesh, cx: f64, cy: f64, shade: f32) {
    build_grass_grid(m, cx, cy, 30, 45.0, shade);
    // Scattered procedural trees outside the spawn clearing.
    let tree_step = 42.0;
    let qx = (cx / tree_step).round() as i64;
    let qy = (cy / tree_step).round() as i64;
    let mut placed = 0usize;
    for dx in -8..=8 {
        for dy in -8..=8 {
            if placed >= 110 {
                return;
            }
            let ix = qx + dx;
            let iy = qy + dy;
            let h = hash2(ENV_SEED ^ 0xF1E1, ix, iy);
            if rand01(h) < 0.58 {
                continue;
            }
            let x = (ix as f64 + (rand01(splitmix64(h)) - 0.5) * 0.65) * tree_step;
            let y = (iy as f64 + (rand01(splitmix64(h ^ 99)) - 0.5) * 0.65) * tree_step;
            if x.hypot(y) < 38.0 {
                continue;
            }
            let th = 4.5 + rand01(h ^ 0xAA) as f32 * 5.0;
            push_tree(m, x, y, th, shade, h);
            placed += 1;
        }
    }
}

fn build_runway(m: &mut WorldMesh, cx: f64, cy: f64, shade: f32) {
    build_grass_grid(m, cx, cy, 30, 45.0, shade);

    let asphalt = scale_col([0.19, 0.20, 0.22, 1.0], shade);
    let asphalt_pad = scale_col([0.23, 0.24, 0.26, 1.0], shade);
    let marking = scale_col([0.92, 0.93, 0.90, 1.0], shade);

    // Main runway (-20..240 m N, -7.5..7.5 m E) and side apron.
    m.push_ned_rect(-20.0, 240.0, -7.5, 7.5, 0.02, asphalt);
    m.push_ned_rect(-20.0, 15.0, 7.5, 22.0, 0.018, asphalt_pad);

    // Edge lines.
    m.push_ned_rect(-18.0, 238.0, -7.25, -6.95, 0.035, marking);
    m.push_ned_rect(-18.0, 238.0, 6.95, 7.25, 0.035, marking);

    // Threshold piano-key markings at both ends.
    for x0 in [-16.0_f64, 228.0_f64] {
        for k in 0..4 {
            let y_off = 1.4 + k as f64 * 1.45;
            m.push_ned_rect(x0, x0 + 8.0, y_off, y_off + 0.75, 0.036, marking);
            m.push_ned_rect(x0, x0 + 8.0, -(y_off + 0.75), -y_off, 0.036, marking);
        }
    }

    // Dashed centreline along y = 0.
    let mut x = -4.0;
    while x < 220.0 {
        m.push_ned_rect(x, (x + 6.0).min(222.0), -0.18, 0.18, 0.036, marking);
        x += 12.0;
    }
}

fn build_city(m: &mut WorldMesh, cx: f64, cy: f64, shade: f32) {
    // Base asphalt street plane.
    let asphalt = scale_col([0.19, 0.20, 0.23, 1.0], shade);
    let sidewalk = scale_col([0.36, 0.37, 0.39, 1.0], shade);
    let park_grass = scale_col([0.24, 0.44, 0.20, 1.0], shade);
    let lane = scale_col([0.82, 0.80, 0.72, 1.0], shade);

    let r_vis = 620.0;
    let qx = (cx / CITY_PITCH).round() * CITY_PITCH;
    let qy = (cy / CITY_PITCH).round() * CITY_PITCH;
    m.push_ned_rect(qx - r_vis, qx + r_vis, qy - r_vis, qy + r_vis, 0.0, asphalt);

    // Open park & runway strip at spawn.
    m.push_ned_rect(-50.0, 220.0, -28.0, 28.0, 0.012, park_grass);
    m.push_ned_rect(-20.0, 210.0, -7.0, 7.0, 0.025, asphalt);
    let mut x = -12.0;
    while x < 204.0 {
        m.push_ned_rect(x, x + 6.0, -0.15, 0.15, 0.038, lane);
        x += 12.0;
    }

    // Collect nearby blocks, sorted closest-first so near buildings get window LOD first.
    let bx0 = ((cx - r_vis) / CITY_PITCH).floor() as i32;
    let bx1 = ((cx + r_vis) / CITY_PITCH).ceil() as i32;
    let by0 = ((cy - r_vis) / CITY_PITCH).floor() as i32;
    let by1 = ((cy + r_vis) / CITY_PITCH).ceil() as i32;

    let mut items: Vec<(f64, i32, i32)> = Vec::new();
    for bx in bx0..=bx1 {
        for by in by0..=by1 {
            let mx = (bx as f64 + 0.5) * CITY_PITCH;
            let my = (by as f64 + 0.5) * CITY_PITCH;
            let d2 = (mx - cx).powi(2) + (my - cy).powi(2);
            if d2 <= r_vis * r_vis {
                items.push((d2, bx, by));
            }
        }
    }
    items.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Sidewalks for near blocks.
    for &(d2, bx, by) in &items {
        if d2 > 420.0 * 420.0 || !m.can_add_tris(400) {
            break;
        }
        let x0 = bx as f64 * CITY_PITCH + STREET_HALF;
        let x1 = (bx + 1) as f64 * CITY_PITCH - STREET_HALF;
        let y0 = by as f64 * CITY_PITCH + STREET_HALF;
        let y1 = (by + 1) as f64 * CITY_PITCH - STREET_HALF;
        if x1 >= -55.0 && x0 <= 225.0 && y1 >= -32.0 && y0 <= 32.0 {
            continue;
        }
        m.push_ned_rect(x0, x1, y0, y1, 0.015, sidewalk);
    }

    // Pass 1: Building boxes (guaranteed for all visible blocks).
    let mut near_buildings: Vec<BuildingBox> = Vec::new();
    for &(d2, bx, by) in &items {
        for b in EnvironmentKind::city_block_buildings(bx, by)
            .into_iter()
            .flatten()
        {
            let tone = rand01(b.seed ^ 0x44) as f32;
            let wall = scale_col(
                [
                    0.46 + 0.16 * tone,
                    0.47 + 0.13 * tone,
                    0.50 + 0.10 * tone,
                    1.0,
                ],
                shade,
            );
            let roof = scale_col([0.34, 0.35, 0.38, 1.0], shade);
            m.push_box_top_sides(
                [b.min_x as f32, -b.max_y as f32, 0.0],
                [b.max_x as f32, -b.min_y as f32, b.height as f32],
                wall,
                roof,
            );
            if d2 < 240.0 * 240.0 {
                near_buildings.push(b);
            }
        }
    }

    // Pass 2: Simple procedural window patterns on nearest buildings (within remaining tri cap).
    let win_dark = scale_col([0.15, 0.24, 0.34, 1.0], shade);
    let win_lit = scale_col([0.85, 0.78, 0.52, 1.0], shade);
    for b in near_buildings {
        if !m.can_add_tris(80) {
            break;
        }
        push_building_windows(m, &b, win_dark, win_lit);
    }
}

fn push_building_windows(
    m: &mut WorldMesh,
    b: &BuildingBox,
    win_dark: [f32; 4],
    win_lit: [f32; 4],
) {
    let x0 = b.min_x as f32;
    let x1 = b.max_x as f32;
    let ry0 = -b.max_y as f32;
    let ry1 = -b.min_y as f32;
    let h = b.height as f32;

    let floors = ((h - 3.0) / 4.2).floor().clamp(1.0, 9.0) as usize;
    let z_step = (h - 2.4) / floors as f32;
    let wh = (z_step * 0.48).min(1.6);
    let off = 0.04_f32;
    let win_col = |r: usize, c: usize| {
        let whash = splitmix64(b.seed ^ ((r as u64) * 31 + c as u64));
        if rand01(whash) > 0.78 {
            win_lit
        } else {
            win_dark
        }
    };

    let cols_x = ((x1 - x0) / 5.0).floor().clamp(2.0, 5.0) as usize;
    let dx = (x1 - x0) / cols_x as f32;
    for r in 0..floors {
        let zb = 2.0 + r as f32 * z_step;
        let zt = zb + wh;
        for c in 0..cols_x {
            if !m.can_add_tris(4) {
                return;
            }
            let col = win_col(r, c);
            let xa = x0 + (c as f32 + 0.22) * dx;
            let xb = x0 + (c as f32 + 0.78) * dx;
            m.push_quad(
                [xa, ry0 - off, zb],
                [xb, ry0 - off, zb],
                [xb, ry0 - off, zt],
                [xa, ry0 - off, zt],
                col,
            );
            m.push_quad(
                [xb, ry1 + off, zb],
                [xa, ry1 + off, zb],
                [xa, ry1 + off, zt],
                [xb, ry1 + off, zt],
                col,
            );
        }
    }

    let cols_y = ((ry1 - ry0) / 5.0).floor().clamp(2.0, 5.0) as usize;
    let dy = (ry1 - ry0) / cols_y as f32;
    for r in 0..floors {
        let zb = 2.0 + r as f32 * z_step;
        let zt = zb + wh;
        for c in 0..cols_y {
            if !m.can_add_tris(4) {
                return;
            }
            let col = win_col(r, c + 16);
            let ya = ry0 + (c as f32 + 0.22) * dy;
            let yb = ry0 + (c as f32 + 0.78) * dy;
            m.push_quad(
                [x1 + off, ya, zb],
                [x1 + off, yb, zb],
                [x1 + off, yb, zt],
                [x1 + off, ya, zt],
                col,
            );
            m.push_quad(
                [x0 - off, yb, zb],
                [x0 - off, ya, zb],
                [x0 - off, ya, zt],
                [x0 - off, yb, zt],
                col,
            );
        }
    }
}

fn build_mountains(m: &mut WorldMesh, cx: f64, cy: f64, shade: f32) {
    const N: usize = 52;
    let snap = 40.0;
    let qx = (cx / snap).round() * snap;
    let qy = (cy / snap).round() * snap;
    let extent = 1200.0;
    let warp = |u: f64| u * (0.28 + 0.72 * u * u) * extent;

    let c_grass = scale_col([0.22, 0.44, 0.19, 1.0], shade);
    let c_rock = scale_col([0.44, 0.41, 0.37, 1.0], shade);
    let c_snow = scale_col([0.88, 0.91, 0.96, 1.0], shade);

    let env = EnvironmentKind::Mountains;
    let mut grid_idx = Vec::with_capacity((N + 1) * (N + 1));
    for iy in 0..=N {
        let uy = (iy as f64 / N as f64) * 2.0 - 1.0;
        let y_ned = qy + warp(uy);
        for ix in 0..=N {
            let ux = (ix as f64 / N as f64) * 2.0 - 1.0;
            let x_ned = qx + warp(ux);
            let h = env.ground_height(x_ned, y_ned);
            let n_ned = env.ground_normal_ned(x_ned, y_ned);
            let n_render = [n_ned.x as f32, -n_ned.y as f32, -n_ned.z as f32];
            let slope = (1.0 - n_render[2] as f64).clamp(0.0, 1.0);
            let rock_t = (((h - 24.0) / 28.0).max((slope - 0.15) / 0.22)).clamp(0.0, 1.0) as f32;
            let snow_slope = (1.0 - (slope - 0.22) / 0.22).clamp(0.0, 1.0);
            let snow_t = (((h - 68.0) / 24.0).clamp(0.0, 1.0) * snow_slope) as f32;
            let base = mix_col(c_grass, c_rock, rock_t);
            let col = mix_col(base, c_snow, snow_t);
            let idx = m.push_vertex([x_ned as f32, -y_ned as f32, h as f32], n_render, col);
            grid_idx.push(idx);
        }
    }
    let stride = N + 1;
    for iy in 0..N {
        for ix in 0..N {
            if !m.can_add_tris(2) {
                break;
            }
            let a = grid_idx[iy * stride + ix];
            let b = grid_idx[iy * stride + ix + 1];
            let c = grid_idx[(iy + 1) * stride + ix + 1];
            let d = grid_idx[(iy + 1) * stride + ix];
            // Render y decreases as y_ned increases, so CCW from +Z is (a, d, c), (a, c, b).
            m.indices.extend_from_slice(&[a, d, c, a, c, b]);
        }
    }

    // Small landing/launch pad marker at the flat spawn valley (0, 0).
    let pad_col = scale_col([0.24, 0.25, 0.27, 1.0], shade);
    let mark_col = scale_col([0.88, 0.88, 0.84, 1.0], shade);
    m.push_ned_rect(-12.0, 36.0, -4.5, 4.5, 0.025, pad_col);
    m.push_ned_rect(-10.0, 34.0, -0.15, 0.15, 0.038, mark_col);
}
