// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tiny 3D math for a mesh viewer. All f32.

/// A 3D vector with x, y, z components.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    /// The zero vector (0, 0, 0).
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Unit vector along the X axis.
    pub const X: Vec3 = Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };

    /// Unit vector along the Y axis.
    pub const Y: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };

    /// Unit vector along the Z axis.
    pub const Z: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    /// Creates a new vector.
    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    /// Dot product with another vector.
    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Cross product with another vector.
    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Euclidean length (magnitude) of the vector.
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Returns the unit vector in the same direction, or ZERO if length is 0 or non-finite.
    pub fn normalized(self) -> Vec3 {
        let len = self.length();
        if len > 0.0 && len.is_finite() {
            Vec3::new(self.x / len, self.y / len, self.z / len)
        } else {
            Vec3::ZERO
        }
    }

    /// True if all components are finite (no NaN or infinity).
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Component-wise minimum with another vector.
    pub fn min(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    /// Component-wise maximum with another vector.
    pub fn max(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    /// Linear interpolation from self to o by factor t.
    pub fn lerp(self, o: Vec3, t: f32) -> Vec3 {
        Vec3::new(
            self.x + (o.x - self.x) * t,
            self.y + (o.y - self.y) * t,
            self.z + (o.z - self.z) * t,
        )
    }
}

impl std::ops::Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl std::ops::AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}

/// Column-major 4x4 matrix (OpenGL convention). cols[c][r].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Mat4 {
    /// The identity matrix.
    pub const IDENTITY: Mat4 = Mat4 {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    /// Creates a translation matrix.
    pub fn translation(t: Vec3) -> Mat4 {
        Mat4 {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [t.x, t.y, t.z, 1.0],
            ],
        }
    }

    /// Creates a scale matrix.
    pub fn scale(s: Vec3) -> Mat4 {
        Mat4 {
            cols: [
                [s.x, 0.0, 0.0, 0.0],
                [0.0, s.y, 0.0, 0.0],
                [0.0, 0.0, s.z, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Rotation around the X axis (right-handed, CCW looking down -X toward origin).
    pub fn rotation_x(angle_rad: f32) -> Mat4 {
        let c = angle_rad.cos();
        let s = angle_rad.sin();
        Mat4 {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, c, s, 0.0],
                [0.0, -s, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Rotation around the Y axis (right-handed, CCW looking down -Y toward origin).
    pub fn rotation_y(angle_rad: f32) -> Mat4 {
        let c = angle_rad.cos();
        let s = angle_rad.sin();
        Mat4 {
            cols: [
                [c, 0.0, -s, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [s, 0.0, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Rotation around the Z axis (right-handed, CCW looking down -Z toward origin).
    pub fn rotation_z(angle_rad: f32) -> Mat4 {
        let c = angle_rad.cos();
        let s = angle_rad.sin();
        Mat4 {
            cols: [
                [c, s, 0.0, 0.0],
                [-s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Right-handed view matrix (camera looks along -Z in view space), like gluLookAt.
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalized();
        let s = f.cross(up).normalized();
        let u = s.cross(f);

        Mat4 {
            cols: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    /// Right-handed perspective projection, OpenGL clip depth -1..1, like gluPerspective.
    pub fn perspective(fovy_rad: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fovy_rad / 2.0).tan();
        let range_inv = 1.0 / (near - far);

        Mat4 {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, (far + near) * range_inv, -1.0],
                [0.0, 0.0, 2.0 * far * near * range_inv, 0.0],
            ],
        }
    }

    /// Right-handed orthographic projection, OpenGL clip depth -1..1, like glOrtho.
    pub fn orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Mat4 {
        Mat4 {
            cols: [
                [2.0 / (right - left), 0.0, 0.0, 0.0],
                [0.0, 2.0 / (top - bottom), 0.0, 0.0],
                [0.0, 0.0, -2.0 / (far - near), 0.0],
                [
                    -(right + left) / (right - left),
                    -(top + bottom) / (top - bottom),
                    -(far + near) / (far - near),
                    1.0,
                ],
            ],
        }
    }

    /// Transforms a point (w=1) and divides by w when w is non-zero.
    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let x =
            self.cols[0][0] * p.x + self.cols[1][0] * p.y + self.cols[2][0] * p.z + self.cols[3][0];
        let y =
            self.cols[0][1] * p.x + self.cols[1][1] * p.y + self.cols[2][1] * p.z + self.cols[3][1];
        let z =
            self.cols[0][2] * p.x + self.cols[1][2] * p.y + self.cols[2][2] * p.z + self.cols[3][2];
        let w =
            self.cols[0][3] * p.x + self.cols[1][3] * p.y + self.cols[2][3] * p.z + self.cols[3][3];

        if w != 0.0 {
            Vec3::new(x / w, y / w, z / w)
        } else {
            Vec3::new(x, y, z)
        }
    }

    /// Transforms a direction (w=0), no translation.
    pub fn transform_vector(&self, v: Vec3) -> Vec3 {
        Vec3::new(
            self.cols[0][0] * v.x + self.cols[1][0] * v.y + self.cols[2][0] * v.z,
            self.cols[0][1] * v.x + self.cols[1][1] * v.y + self.cols[2][1] * v.z,
            self.cols[0][2] * v.x + self.cols[1][2] * v.y + self.cols[2][2] * v.z,
        )
    }

    /// Flat column-major array for glUniformMatrix4fv.
    pub fn to_cols_array(&self) -> [f32; 16] {
        let mut arr = [0.0f32; 16];
        for c in 0..4 {
            for r in 0..4 {
                arr[c * 4 + r] = self.cols[c][r];
            }
        }
        arr
    }
}

impl std::ops::Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, rhs: Mat4) -> Mat4 {
        // Standard matrix product: (self * rhs)[c][r] = sum_k self[k][r] * rhs[c][k]
        let mut result = Mat4::IDENTITY;
        for c in 0..4 {
            for r in 0..4 {
                let mut sum = 0.0f32;
                for k in 0..4 {
                    sum += self.cols[k][r] * rhs.cols[c][k];
                }
                result.cols[c][r] = sum;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    fn approx_vec3(a: Vec3, b: Vec3) -> bool {
        approx(a.x, b.x) && approx(a.y, b.y) && approx(a.z, b.z)
    }

    #[test]
    fn cross_x_y_is_z() {
        assert_eq!(Vec3::X.cross(Vec3::Y), Vec3::Z);
    }

    #[test]
    fn normalized_zero_is_zero() {
        assert_eq!(Vec3::ZERO.normalized(), Vec3::ZERO);
    }

    #[test]
    fn normalized_nan_vector_is_zero() {
        let nan_vec = Vec3::new(f32::NAN, f32::NAN, f32::NAN);
        assert_eq!(nan_vec.normalized(), Vec3::ZERO);
    }

    #[test]
    fn translation_moves_point() {
        let m = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        let p = Vec3::new(10.0, 20.0, 30.0);
        let result = m.transform_point(p);
        assert!(approx_vec3(result, Vec3::new(11.0, 22.0, 33.0)));
    }

    #[test]
    fn rotation_z_pi_over_2_maps_x_to_y() {
        let m = Mat4::rotation_z(std::f32::consts::PI / 2.0);
        let result = m.transform_point(Vec3::X);
        assert!(approx_vec3(result, Vec3::Y));
    }

    #[test]
    fn identity_times_m_is_m() {
        let m = Mat4::translation(Vec3::new(1.0, 2.0, 3.0));
        let result = Mat4::IDENTITY * m;
        assert!(approx(result.cols[0][0], m.cols[0][0]));
        assert!(approx(result.cols[3][2], m.cols[3][2]));
    }

    #[test]
    fn chained_transform_point() {
        let a = Mat4::translation(Vec3::new(1.0, 0.0, 0.0));
        let b = Mat4::scale(Vec3::new(2.0, 2.0, 2.0));
        let p = Vec3::new(1.0, 1.0, 1.0);

        let ab = a * b;
        let combined = ab.transform_point(p);
        let sequential = a.transform_point(b.transform_point(p));

        assert!(approx_vec3(combined, sequential));
    }

    #[test]
    fn look_at_eye_5_z_to_origin() {
        let eye = Vec3::new(0.0, 0.0, 5.0);
        let target = Vec3::ZERO;
        let up = Vec3::Y;
        let m = Mat4::look_at(eye, target, up);
        let result = m.transform_point(Vec3::ZERO);
        assert!(approx_vec3(result, Vec3::new(0.0, 0.0, -5.0)));
    }

    #[test]
    fn perspective_near_and_far_centers() {
        let near = 0.1;
        let far = 100.0;
        let m = Mat4::perspective(std::f32::consts::FRAC_PI_4, 1.0, near, far);

        let near_pt = m.transform_point(Vec3::new(0.0, 0.0, -near));
        let far_pt = m.transform_point(Vec3::new(0.0, 0.0, -far));

        assert!(approx(near_pt.z, -1.0));
        assert!(approx(far_pt.z, 1.0));
    }

    #[test]
    fn orthographic_maps_corner() {
        let left = -2.0;
        let right = 2.0;
        let bottom = -1.5;
        let top = 1.5;
        let near = 0.5;
        let far = 5.0;
        let m = Mat4::orthographic(left, right, bottom, top, near, far);

        let p = Vec3::new(left, bottom, -near);
        let result = m.transform_point(p);
        assert!(approx(result.x, -1.0));
        assert!(approx(result.y, -1.0));
        assert!(approx(result.z, -1.0));
    }

    #[test]
    fn to_cols_array_translation_indices() {
        let t = Vec3::new(1.5, 2.5, 3.5);
        let m = Mat4::translation(t);
        let arr = m.to_cols_array();
        assert!(approx(arr[12], 1.5));
        assert!(approx(arr[13], 2.5));
        assert!(approx(arr[14], 3.5));
    }
}
