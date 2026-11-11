// SPDX-License-Identifier: AGPL-3.0-or-later
//! 3D önizleme için saf geometri: küçük vektör/matris matematiği, üçgen mesh ve temel
//! şekiller, uçak parametrelerinden sahne üretimi ve zemin ızgarası. egui/GL bağımlılığı yoktur;
//! görüntüleme `ui::viewport3d` içindedir.
pub mod aircraft;
pub mod grid;
pub mod math;
pub mod mesh;
pub mod spec;

pub use aircraft::{AircraftSpec, Part, PartKind, Scene, build};
pub use math::{Mat4, Vec3};
pub use mesh::Mesh;
pub use spec::{ViewOptions, from_project};
