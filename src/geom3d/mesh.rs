// SPDX-License-Identifier: AGPL-3.0-or-later
//! Üçgen mesh kabı ve temel şekil üreticileri (kutu, silindir, disk, dönel yüzey).
use super::math::Vec3;

/// Triangle mesh container: vertex positions, normals, and triangle indices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>, // same length as positions, unit length (or zero)
    pub indices: Vec<u32>,      // triangles, counter-clockwise when seen from outside
}

impl Mesh {
    /// Creates an empty mesh.
    pub fn new() -> Mesh {
        Mesh::default()
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Number of triangles (indices must be a multiple of 3).
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// True if the mesh has no triangles.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Adds a vertex (position + normal), returns its index.
    pub fn push_vertex(&mut self, p: Vec3, n: Vec3) -> u32 {
        let idx = self.positions.len() as u32;
        self.positions.push([p.x, p.y, p.z]);
        self.normals.push([n.x, n.y, n.z]);
        idx
    }

    /// Pushes one triangle (a, b, c) into the index buffer.
    pub fn push_triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.push(a);
        self.indices.push(b);
        self.indices.push(c);
    }

    /// Appends another mesh, offsetting its indices.
    pub fn append(&mut self, other: &Mesh) {
        let offset = self.positions.len() as u32;
        self.positions.extend_from_slice(&other.positions);
        self.normals.extend_from_slice(&other.normals);
        for &idx in &other.indices {
            self.indices.push(idx + offset);
        }
    }

    /// Axis-aligned bounding box (min, max); None if no vertices.
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        if self.positions.is_empty() {
            return None;
        }
        let mut min = self.positions[0];
        let mut max = self.positions[0];
        for &p in &self.positions {
            min = [min[0].min(p[0]), min[1].min(p[1]), min[2].min(p[2])];
            max = [max[0].max(p[0]), max[1].max(p[1]), max[2].max(p[2])];
        }
        Some((
            Vec3::new(min[0], min[1], min[2]),
            Vec3::new(max[0], max[1], max[2]),
        ))
    }

    /// Validates finite components and index ranges.
    pub fn is_valid(&self) -> bool {
        let vc = self.positions.len();
        if vc == 0 && self.indices.is_empty() {
            return true;
        }
        if self.normals.len() != vc {
            return false;
        }
        if !self.indices.len().is_multiple_of(3) {
            return false;
        }
        for &p in &self.positions {
            if !p.iter().all(|v| v.is_finite()) {
                return false;
            }
        }
        for &n in &self.normals {
            if !n.iter().all(|v| v.is_finite()) {
                return false;
            }
        }
        for &idx in &self.indices {
            if idx as usize >= vc {
                return false;
            }
        }
        true
    }

    /// Removes triangles referencing any vertex with a non-finite position.
    pub fn drop_non_finite_triangles(&mut self) {
        let vc = self.positions.len();
        let bad: Vec<bool> = (0..vc)
            .map(|i| !self.positions[i].iter().all(|v| v.is_finite()))
            .collect();
        let mut new_indices = Vec::with_capacity(self.indices.len());
        for t in (0..self.indices.len()).step_by(3) {
            let a = self.indices[t] as usize;
            let b = self.indices[t + 1] as usize;
            let c = self.indices[t + 2] as usize;
            if a < vc && b < vc && c < vc && !bad[a] && !bad[b] && !bad[c] {
                new_indices.push(self.indices[t]);
                new_indices.push(self.indices[t + 1]);
                new_indices.push(self.indices[t + 2]);
            }
        }
        self.indices = new_indices;
        self.compact();
    }

    /// Removes vertices no triangle references (keeps index order).
    pub fn compact(&mut self) {
        let vc = self.positions.len();
        let mut remap = vec![u32::MAX; vc];
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        for idx in &mut self.indices {
            let i = *idx as usize;
            if i >= vc {
                continue;
            }
            if remap[i] == u32::MAX {
                remap[i] = pos.len() as u32;
                pos.push(self.positions[i]);
                nrm.push(self.normals.get(i).copied().unwrap_or([0.0, 0.0, 1.0]));
            }
            *idx = remap[i];
        }
        self.positions = pos;
        self.normals = nrm;
    }
}

/// Flat-shaded quad a-b-c-d (counter-clockwise), normal from (b-a)x(c-a).
pub fn push_quad(mesh: &mut Mesh, a: Vec3, b: Vec3, c: Vec3, d: Vec3) {
    let n = (b - a).cross(c - a).normalized();
    let i0 = mesh.push_vertex(a, n);
    let i1 = mesh.push_vertex(b, n);
    let i2 = mesh.push_vertex(c, n);
    let i3 = mesh.push_vertex(d, n);
    mesh.push_triangle(i0, i1, i2);
    mesh.push_triangle(i0, i2, i3);
}

/// Axis-aligned box centred at `center` with full size `size`. 24 vertices, 12 triangles.
pub fn push_box(mesh: &mut Mesh, center: Vec3, size: Vec3) {
    let hx = size.x * 0.5;
    let hy = size.y * 0.5;
    let hz = size.z * 0.5;

    // Each face: 4 vertices (a, b, c, d in CCW seen from outside) + 2 triangles.
    macro_rules! face {
        ($n:expr, $pa:expr, $pb:expr, $pc:expr, $pd:expr) => {{
            let i0 = mesh.push_vertex($pa, $n);
            let i1 = mesh.push_vertex($pb, $n);
            let i2 = mesh.push_vertex($pc, $n);
            let i3 = mesh.push_vertex($pd, $n);
            // Sarımı normale göre seç (dışa bakan yüz saat yönünün tersi).
            if ($pb - $pa).cross($pc - $pa).dot($n) >= 0.0 {
                mesh.push_triangle(i0, i1, i2);
                mesh.push_triangle(i0, i2, i3);
            } else {
                mesh.push_triangle(i0, i2, i1);
                mesh.push_triangle(i0, i3, i2);
            }
        }};
    }

    // +X face (normal = +X): CCW seen from +X.
    face!(
        Vec3::X,
        Vec3::new(center.x + hx, center.y - hy, center.z + hz),
        Vec3::new(center.x + hx, center.y - hy, center.z - hz),
        Vec3::new(center.x + hx, center.y + hy, center.z - hz),
        Vec3::new(center.x + hx, center.y + hy, center.z + hz)
    );
    // -X face (normal = -X)
    face!(
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(center.x - hx, center.y - hy, center.z - hz),
        Vec3::new(center.x - hx, center.y - hy, center.z + hz),
        Vec3::new(center.x - hx, center.y + hy, center.z + hz),
        Vec3::new(center.x - hx, center.y + hy, center.z - hz)
    );
    // +Y face
    face!(
        Vec3::Y,
        Vec3::new(center.x - hx, center.y + hy, center.z + hz),
        Vec3::new(center.x + hx, center.y + hy, center.z + hz),
        Vec3::new(center.x + hx, center.y + hy, center.z - hz),
        Vec3::new(center.x - hx, center.y + hy, center.z - hz)
    );
    // -Y face
    face!(
        Vec3::new(0.0, -1.0, 0.0),
        Vec3::new(center.x - hx, center.y - hy, center.z - hz),
        Vec3::new(center.x + hx, center.y - hy, center.z - hz),
        Vec3::new(center.x + hx, center.y - hy, center.z + hz),
        Vec3::new(center.x - hx, center.y - hy, center.z + hz)
    );
    // +Z face
    face!(
        Vec3::Z,
        Vec3::new(center.x - hx, center.y - hy, center.z + hz),
        Vec3::new(center.x - hx, center.y + hy, center.z + hz),
        Vec3::new(center.x + hx, center.y + hy, center.z + hz),
        Vec3::new(center.x + hx, center.y - hy, center.z + hz)
    );
    // -Z face
    face!(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(center.x - hx, center.y - hy, center.z - hz),
        Vec3::new(center.x + hx, center.y - hy, center.z - hz),
        Vec3::new(center.x + hx, center.y + hy, center.z - hz),
        Vec3::new(center.x - hx, center.y + hy, center.z - hz)
    );
}

/// Builds an orthonormal basis (u, v) perpendicular to unit direction d.
fn basis_around(d: Vec3) -> (Vec3, Vec3) {
    let helper = if d.z.abs() > 0.9 { Vec3::X } else { Vec3::Z };
    let u = d.cross(helper).normalized();
    let v = d.cross(u);
    (u, v)
}

/// Cylinder between points `a` and `b`, radius `r`, `segments` around (min 3).
/// Side has smooth radial normals; with `caps` flat discs close both ends.
/// Degenerate or non-finite input adds nothing.
pub fn push_cylinder(mesh: &mut Mesh, a: Vec3, b: Vec3, r: f32, segments: u32, caps: bool) {
    let dir = b - a;
    if !a.is_finite() || !dir.is_finite() || dir.length() <= 1e-9 || !(r.is_finite() && r > 0.0) {
        return;
    }
    let seg = segments.max(3);
    let d = dir.normalized();
    let (u, v) = basis_around(d);
    let first = mesh.vertex_count() as u32;
    for i in 0..seg {
        let t = std::f32::consts::TAU * (i as f32 / seg as f32);
        let radial = u * t.cos() + v * t.sin();
        mesh.push_vertex(a + radial * r, radial);
        mesh.push_vertex(b + radial * r, radial);
    }
    for i in 0..seg {
        let j = (i + 1) % seg;
        let (a0, b0) = (first + 2 * i, first + 2 * i + 1);
        let (a1, b1) = (first + 2 * j, first + 2 * j + 1);
        // Yüz normali (a1-a0)x(b1-a0) dışa bakacak şekilde sarım seçilir.
        let p = |k: u32| {
            let q = mesh.positions[k as usize];
            Vec3::new(q[0], q[1], q[2])
        };
        let f = (p(a1) - p(a0)).cross(p(b1) - p(a0));
        let radial = {
            let q = mesh.normals[a0 as usize];
            Vec3::new(q[0], q[1], q[2])
        };
        if f.dot(radial) >= 0.0 {
            mesh.push_triangle(a0, a1, b1);
            mesh.push_triangle(a0, b1, b0);
        } else {
            mesh.push_triangle(a0, b1, a1);
            mesh.push_triangle(a0, b0, b1);
        }
    }
    if caps {
        push_cap(mesh, a, -d, r, seg, u, v);
        push_cap(mesh, b, d, r, seg, u, v);
    }
}

/// Pushes a flat disc cap at center with given normal and radius.
fn push_cap(mesh: &mut Mesh, center: Vec3, normal: Vec3, r: f32, seg: u32, u: Vec3, v: Vec3) {
    if r <= 0.0 || !r.is_finite() {
        return;
    }
    let center_idx = mesh.push_vertex(center, normal);
    for i in 0..seg {
        let i0 = i as usize;
        let i1 = ((i + 1) % seg) as usize;
        let t0 = 2.0 * std::f32::consts::PI * (i0 as f32 / seg as f32);
        let t1 = 2.0 * std::f32::consts::PI * (i1 as f32 / seg as f32);
        let p0 = center + (u * t0.cos() + v * t0.sin()) * r;
        let p1 = center + (u * t1.cos() + v * t1.sin()) * r;
        let i0v = mesh.push_vertex(p0, normal);
        let i1v = mesh.push_vertex(p1, normal);
        if (p0 - center).cross(p1 - center).dot(normal) >= 0.0 {
            mesh.push_triangle(center_idx, i0v, i1v);
        } else {
            mesh.push_triangle(center_idx, i1v, i0v);
        }
    }
}

/// Flat disc centred at center, perpendicular to unit normal, radius r.
/// Both faces visible.
pub fn push_disc(mesh: &mut Mesh, center: Vec3, normal: Vec3, r: f32, segments: u32) {
    if r <= 0.0 || !r.is_finite() || !normal.is_finite() {
        return;
    }
    let n = if normal.length() == 0.0 {
        Vec3::Z
    } else {
        normal.normalized()
    };
    let seg = segments.max(3);
    let (u, v) = basis_around(n);

    // Front face (normal = n)
    let cf = mesh.push_vertex(center, n);
    for i in 0..seg {
        let i0 = i as usize;
        let i1 = ((i + 1) % seg) as usize;
        let t0 = 2.0 * std::f32::consts::PI * (i0 as f32 / seg as f32);
        let t1 = 2.0 * std::f32::consts::PI * (i1 as f32 / seg as f32);
        let p0 = center + (u * t0.cos() + v * t0.sin()) * r;
        let p1 = center + (u * t1.cos() + v * t1.sin()) * r;
        let i0v = mesh.push_vertex(p0, n);
        let i1v = mesh.push_vertex(p1, n);
        mesh.push_triangle(cf, i0v, i1v);
    }

    // Back face (normal = -n), opposite winding
    let cb = mesh.push_vertex(center, -n);
    for i in 0..seg {
        let i0 = i as usize;
        let i1 = ((i + 1) % seg) as usize;
        let t0 = 2.0 * std::f32::consts::PI * (i0 as f32 / seg as f32);
        let t1 = 2.0 * std::f32::consts::PI * (i1 as f32 / seg as f32);
        let p0 = center + (u * t0.cos() + v * t0.sin()) * r;
        let p1 = center + (u * t1.cos() + v * t1.sin()) * r;
        let i0v = mesh.push_vertex(p0, -n);
        let i1v = mesh.push_vertex(p1, -n);
        mesh.push_triangle(cb, i1v, i0v);
    }
}

/// Surface of revolution around the +X axis. `profile` is (x, radius) pairs ordered by x.
/// Radius 0 at an end closes the shape there (single apex vertex). Smooth normals from the
/// profile slope. Fewer than 2 points, a negative radius or any non-finite value adds nothing.
pub fn push_lathe_x(mesh: &mut Mesh, profile: &[(f32, f32)], segments: u32) {
    if profile.len() < 2
        || profile
            .iter()
            .any(|&(x, r)| !x.is_finite() || !r.is_finite() || r < 0.0)
    {
        return;
    }
    let seg = segments.max(3);
    let n = profile.len();
    // Profil düzleminde (x, r) dış normal: teğete (dx, dr) dik → (-dr, dx), r bileşeni ≥ 0.
    let normal2 = |i: usize| -> (f32, f32) {
        let (x0, r0) = profile[i.saturating_sub(1)];
        let (x1, r1) = profile[(i + 1).min(n - 1)];
        let (dx, dr) = (x1 - x0, r1 - r0);
        let l = (dx * dx + dr * dr).sqrt();
        if l > 1e-12 {
            let (mut nx, mut nr) = (-dr / l, dx / l);
            if nr < 0.0 {
                nx = -nx;
                nr = -nr;
            }
            (nx, nr)
        } else {
            (0.0, 1.0)
        }
    };
    let mut rings: Vec<(u32, bool)> = Vec::with_capacity(n);
    for (i, &(x, r)) in profile.iter().enumerate() {
        let (nx, nr) = normal2(i);
        let first = mesh.vertex_count() as u32;
        if r <= 0.0 {
            let dir = if i == 0 { -1.0 } else { 1.0 };
            mesh.push_vertex(Vec3::new(x, 0.0, 0.0), Vec3::new(dir, 0.0, 0.0));
            rings.push((first, true));
        } else {
            for s in 0..seg {
                let t = std::f32::consts::TAU * (s as f32 / seg as f32);
                let (c, sn) = (t.cos(), t.sin());
                mesh.push_vertex(
                    Vec3::new(x, r * c, r * sn),
                    Vec3::new(nx, nr * c, nr * sn).normalized(),
                );
            }
            rings.push((first, false));
        }
    }
    let idx = |ring: (u32, bool), s: u32| if ring.1 { ring.0 } else { ring.0 + (s % seg) };
    let pos = |m: &Mesh, k: u32| {
        let q = m.positions[k as usize];
        Vec3::new(q[0], q[1], q[2])
    };
    for w in rings.windows(2) {
        let (ra, rb) = (w[0], w[1]);
        if ra.1 && rb.1 {
            continue;
        }
        for s in 0..seg {
            let (a0, a1) = (idx(ra, s), idx(ra, s + 1));
            let (b0, b1) = (idx(rb, s), idx(rb, s + 1));
            let tris: [(u32, u32, u32); 2] = [(a0, a1, b1), (a0, b1, b0)];
            for (p0, p1, p2) in tris {
                if p0 == p1 || p1 == p2 || p0 == p2 {
                    continue;
                }
                let f = (pos(mesh, p1) - pos(mesh, p0)).cross(pos(mesh, p2) - pos(mesh, p0));
                if f.length() < 1e-14 {
                    continue;
                }
                // Dışa bakan yön: köşelerin radyal yönlerinin toplamı.
                let c = pos(mesh, p0) + pos(mesh, p1) + pos(mesh, p2);
                let out = Vec3::new(0.0, c.y, c.z);
                if f.dot(out) >= 0.0 {
                    mesh.push_triangle(p0, p1, p2);
                } else {
                    mesh.push_triangle(p0, p2, p1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-4
    }

    /// Her üçgenin yüz normali köşe normalleriyle aynı yarı uzayda mı (dışa bakan sarım)?
    fn winding_matches_normals(m: &Mesh) -> bool {
        m.indices.as_chunks::<3>().0.iter().all(|t| {
            let p = |i: u32| {
                let v = m.positions[i as usize];
                Vec3::new(v[0], v[1], v[2])
            };
            let n = |i: u32| {
                let v = m.normals[i as usize];
                Vec3::new(v[0], v[1], v[2])
            };
            let f = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
            f.length() < 1e-12 || f.dot(n(t[0]) + n(t[1]) + n(t[2])) > 0.0
        })
    }

    #[test]
    fn box_counts_bounds_and_unit_normals() {
        let mut m = Mesh::new();
        push_box(&mut m, Vec3::new(1.0, 2.0, 3.0), Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(m.vertex_count(), 24);
        assert_eq!(m.triangle_count(), 12);
        let (a, b) = m.bounds().unwrap();
        assert_eq!(a, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(b, Vec3::new(2.0, 4.0, 6.0));
        for n in &m.normals {
            assert!(approx(Vec3::new(n[0], n[1], n[2]).length(), 1.0));
        }
        assert!(winding_matches_normals(&m));
    }

    #[test]
    fn cylinder_bounds_valid_and_outward() {
        let mut m = Mesh::new();
        push_cylinder(&mut m, Vec3::ZERO, Vec3::new(0.0, 0.0, 2.0), 0.5, 16, true);
        let (a, b) = m.bounds().unwrap();
        assert!(approx(a.z, 0.0) && approx(b.z, 2.0));
        assert!(approx(a.x, -0.5) && approx(b.x, 0.5));
        assert!(m.is_valid());
        assert!(winding_matches_normals(&m));
        let mut m = Mesh::new();
        push_cylinder(
            &mut m,
            Vec3::new(0.1, 0.2, 0.3),
            Vec3::new(1.0, -0.5, 0.2),
            0.05,
            12,
            true,
        );
        assert!(winding_matches_normals(&m));
    }

    #[test]
    fn degenerate_cylinder_adds_nothing() {
        let mut m = Mesh::new();
        push_cylinder(&mut m, Vec3::X, Vec3::X, 0.5, 16, true);
        push_cylinder(&mut m, Vec3::ZERO, Vec3::X, 0.0, 16, true);
        push_cylinder(
            &mut m,
            Vec3::ZERO,
            Vec3::new(f32::NAN, 0.0, 0.0),
            0.1,
            16,
            true,
        );
        assert!(m.is_empty() && m.vertex_count() == 0);
    }

    #[test]
    fn disc_is_two_sided() {
        let mut m = Mesh::new();
        push_disc(&mut m, Vec3::ZERO, Vec3::Z, 1.0, 24);
        assert_eq!(m.triangle_count(), 48);
        assert!(winding_matches_normals(&m));
        let mut m2 = Mesh::new();
        push_disc(&mut m2, Vec3::ZERO, Vec3::Z, 0.0, 24);
        assert!(m2.is_empty());
    }

    #[test]
    fn lathe_bounds_and_outward() {
        let mut m = Mesh::new();
        push_lathe_x(&mut m, &[(0.0, 0.0), (1.0, 0.5), (2.0, 0.0)], 24);
        let (a, b) = m.bounds().unwrap();
        assert!(approx(a.x, 0.0) && approx(b.x, 2.0));
        assert!(approx(b.y, 0.5));
        assert!(m.is_valid());
        assert!(winding_matches_normals(&m));
        let mut m = Mesh::new();
        push_lathe_x(&mut m, &[(0.0, 0.1), (f32::NAN, 0.5)], 24);
        assert!(m.is_empty());
    }

    #[test]
    fn append_offsets_indices() {
        let mut a = Mesh::new();
        push_box(&mut a, Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let b = a.clone();
        a.append(&b);
        assert_eq!(*a.indices.iter().max().unwrap(), 47);
        assert!(a.is_valid());
    }

    #[test]
    fn drop_non_finite_removes_touching_triangles_and_compacts() {
        let mut m = Mesh::new();
        let a = m.push_vertex(Vec3::ZERO, Vec3::Z);
        let b = m.push_vertex(Vec3::X, Vec3::Z);
        let c = m.push_vertex(Vec3::Y, Vec3::Z);
        let d = m.push_vertex(Vec3::new(f32::NAN, 0.0, 0.0), Vec3::Z);
        m.push_triangle(a, b, c);
        m.push_triangle(a, c, d);
        m.drop_non_finite_triangles();
        assert_eq!(m.triangle_count(), 1);
        assert!(m.is_valid());
    }
}
