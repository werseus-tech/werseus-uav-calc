// SPDX-License-Identifier: AGPL-3.0-or-later
//! eframe/glow üzerinden küçük mesh çizici (`egui::PaintCallback` içinde çalışır).
//!
//! Tasarım:
//! - Kendi çok örneklemeli (MSAA) çerçeve tamponuna çizer, sonra görünüm dikdörtgenine kopyalar
//!   (blit). Böylece ana pencerenin derinlik tamponuna ve MSAA ayarına bağımlı değildir; kenarlar
//!   yumuşak görünür.
//! - Tek gölgelendirici: yarım küre ortam ışığı + kameraya bağlı yönlü ışık + hafif parlama,
//!   iki yüzlü aydınlatma (pervane diskleri ve ince yüzeyler arkadan da görünür).
//! - Herhangi bir adım başarısız olursa `failed` bayrağı kalkar; arayüz bir sonraki karede
//!   sessizce 2D şemaya düşer.
//!
//! GL durumu: egui her geri çağrıdan sonra kendi durumunu yeniden kurar (`prepare_painting`);
//! burada ayrıca değiştirdiğimiz her şeyi (çerçeve tamponu, derinlik, makas, VAO) geri alırız.
use crate::geom3d::{PartKind, Scene};
use eframe::glow::{self, HasContext};
use std::sync::Arc;

/// Bir karede çizim için gereken her şey (UI iş parçacığında hazırlanır, kopyalanır).
#[derive(Clone)]
pub struct FrameData {
    pub view_proj: [f32; 16],
    pub eye: [f32; 3],
    pub light_dir: [f32; 3],
    pub clear: [f32; 4],
    pub grid_minor: [f32; 4],
    pub grid_major: [f32; 4],
    /// Tür → renk (RGBA, 0..1). Listede olmayan tür çizilmez.
    pub colors: Vec<(PartKind, [f32; 4])>,
    pub scene: Arc<Scene>,
    pub scene_version: u64,
    /// Zemin ızgarası çizgileri (x,y,z çiftleri) ve ana çizgi ayrımı.
    pub grid: Arc<GridData>,
}

#[derive(Default)]
pub struct GridData {
    pub minor: Vec<[f32; 3]>,
    pub major: Vec<[f32; 3]>,
    pub version: u64,
}

struct GpuMesh {
    kind: PartKind,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    count: i32,
}

struct GpuLines {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    minor: i32,
    major: i32,
}

struct Target {
    fbo: glow::Framebuffer,
    color: glow::Renderbuffer,
    depth: glow::Renderbuffer,
    /// MSAA kullanılıyorsa örneklemesiz ara tampon: önce buraya çözülür (aynı biçim), sonra
    /// pencereye kopyalanır. Böylece pencere tamponunun biçimi (RGB8/sRGB) farklı olsa da
    /// çok örnekli kopyalama kuralı (biçimler aynı olmalı) çiğnenmez.
    resolve: Option<(glow::Framebuffer, glow::Renderbuffer)>,
    w: i32,
    h: i32,
}

pub struct Renderer {
    program: glow::Program,
    u_vp: Option<glow::UniformLocation>,
    u_color: Option<glow::UniformLocation>,
    u_light: Option<glow::UniformLocation>,
    u_eye: Option<glow::UniformLocation>,
    u_lit: Option<glow::UniformLocation>,
    a_pos: u32,
    a_nrm: Option<u32>,
    meshes: Vec<GpuMesh>,
    scene_version: u64,
    lines: Option<GpuLines>,
    grid_version: u64,
    target: Option<Target>,
    samples: i32,
}

const VS: &str = r#"
precision highp float;
in vec3 a_pos;
in vec3 a_nrm;
uniform mat4 u_vp;
out vec3 v_nrm;
out vec3 v_pos;
void main() {
    v_nrm = a_nrm;
    v_pos = a_pos;
    gl_Position = u_vp * vec4(a_pos, 1.0);
}
"#;

const FS: &str = r#"
precision highp float;
in vec3 v_nrm;
in vec3 v_pos;
uniform vec4 u_color;
uniform vec3 u_light;
uniform vec3 u_eye;
uniform float u_lit;
out vec4 o_color;
void main() {
    vec3 base = u_color.rgb;
    vec3 c = base;
    if (u_lit > 0.5) {
        vec3 n = normalize(v_nrm);
        vec3 v = normalize(u_eye - v_pos);
        if (dot(n, v) < 0.0) { n = -n; }
        float diff = max(dot(n, u_light), 0.0);
        float hemi = 0.5 + 0.5 * n.z;
        vec3 h = normalize(u_light + v);
        float spec = pow(max(dot(n, h), 0.0), 40.0) * 0.22;
        float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.12;
        c = base * (0.30 + 0.20 * hemi + 0.58 * diff) + vec3(spec + rim);
    }
    o_color = vec4(c, u_color.a);
}
"#;

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

fn u32_bytes(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_ne_bytes()).collect()
}

impl Renderer {
    /// Gölgelendiriciyi derler. Eski (GLSL 1.20/ES 1.00) sürümlerde `Err` döner → 2D'ye düşülür.
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        let sv = eframe::egui_glow::ShaderVersion::get(gl);
        if !sv.is_new_shader_interface() {
            return Err("GLSL 1.30+ gerekli".into());
        }
        let header = sv.version_declaration();
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, &format!("{header}{src}"));
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    let log = gl.get_shader_info_log(sh);
                    gl.delete_shader(sh);
                    gl.delete_program(program);
                    return Err(log);
                }
                gl.attach_shader(program, sh);
                shaders.push(sh);
            }
            gl.link_program(program);
            for sh in shaders {
                gl.detach_shader(program, sh);
                gl.delete_shader(sh);
            }
            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(log);
            }
            let a_pos = gl
                .get_attrib_location(program, "a_pos")
                .ok_or_else(|| "a_pos yok".to_string())?;
            let a_nrm = gl.get_attrib_location(program, "a_nrm");
            let max_samples = gl.get_parameter_i32(glow::MAX_SAMPLES);
            // Bazı sürücüler hata kodu bırakır; temizle.
            while gl.get_error() != glow::NO_ERROR {}
            Ok(Self {
                u_vp: gl.get_uniform_location(program, "u_vp"),
                u_color: gl.get_uniform_location(program, "u_color"),
                u_light: gl.get_uniform_location(program, "u_light"),
                u_eye: gl.get_uniform_location(program, "u_eye"),
                u_lit: gl.get_uniform_location(program, "u_lit"),
                program,
                a_pos,
                a_nrm,
                meshes: Vec::new(),
                scene_version: u64::MAX,
                lines: None,
                grid_version: u64::MAX,
                target: None,
                samples: max_samples.clamp(0, 4),
            })
        }
    }

    unsafe fn bind_attribs(&self, gl: &glow::Context, stride: i32, with_normal: bool) {
        unsafe {
            gl.enable_vertex_attrib_array(self.a_pos);
            gl.vertex_attrib_pointer_f32(self.a_pos, 3, glow::FLOAT, false, stride, 0);
            if let Some(n) = self.a_nrm {
                if with_normal {
                    gl.enable_vertex_attrib_array(n);
                    gl.vertex_attrib_pointer_f32(n, 3, glow::FLOAT, false, stride, 12);
                } else {
                    gl.disable_vertex_attrib_array(n);
                    gl.vertex_attrib_3_f32(n, 0.0, 0.0, 1.0);
                }
            }
        }
    }

    fn free_meshes(&mut self, gl: &glow::Context) {
        unsafe {
            for m in self.meshes.drain(..) {
                gl.delete_vertex_array(m.vao);
                gl.delete_buffer(m.vbo);
                gl.delete_buffer(m.ibo);
            }
        }
    }

    fn upload_scene(&mut self, gl: &glow::Context, scene: &Scene) -> Result<(), String> {
        self.free_meshes(gl);
        unsafe {
            for part in &scene.parts {
                let m = &part.mesh;
                if m.is_empty() || !m.is_valid() {
                    continue;
                }
                let mut inter = Vec::with_capacity(m.positions.len() * 6);
                for (p, n) in m.positions.iter().zip(&m.normals) {
                    inter.extend_from_slice(p);
                    inter.extend_from_slice(n);
                }
                let vao = gl.create_vertex_array()?;
                gl.bind_vertex_array(Some(vao));
                let vbo = gl.create_buffer()?;
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &f32_bytes(&inter), glow::STATIC_DRAW);
                let ibo = gl.create_buffer()?;
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ibo));
                gl.buffer_data_u8_slice(
                    glow::ELEMENT_ARRAY_BUFFER,
                    &u32_bytes(&m.indices),
                    glow::STATIC_DRAW,
                );
                self.bind_attribs(gl, 24, true);
                gl.bind_vertex_array(None);
                self.meshes.push(GpuMesh {
                    kind: part.kind,
                    vao,
                    vbo,
                    ibo,
                    count: m.indices.len() as i32,
                });
            }
        }
        Ok(())
    }

    fn upload_grid(&mut self, gl: &glow::Context, g: &GridData) -> Result<(), String> {
        unsafe {
            if let Some(l) = self.lines.take() {
                gl.delete_vertex_array(l.vao);
                gl.delete_buffer(l.vbo);
            }
            if g.minor.is_empty() && g.major.is_empty() {
                return Ok(());
            }
            let mut data = Vec::with_capacity((g.minor.len() + g.major.len()) * 3);
            for p in g.minor.iter().chain(&g.major) {
                data.extend_from_slice(p);
            }
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &f32_bytes(&data), glow::STATIC_DRAW);
            self.bind_attribs(gl, 12, false);
            gl.bind_vertex_array(None);
            self.lines = Some(GpuLines {
                vao,
                vbo,
                minor: g.minor.len() as i32,
                major: g.major.len() as i32,
            });
        }
        Ok(())
    }

    fn ensure_target(&mut self, gl: &glow::Context, w: i32, h: i32) -> Result<(), String> {
        if let Some(t) = &self.target {
            if t.w == w && t.h == h {
                return Ok(());
            }
        }
        unsafe {
            self.free_target(gl);
            // Önce MSAA dene; olmazsa örneklemesiz.
            for samples in [self.samples, 0] {
                let fbo = gl.create_framebuffer()?;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                let color = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
                if samples > 0 {
                    gl.renderbuffer_storage_multisample(
                        glow::RENDERBUFFER,
                        samples,
                        glow::RGBA8,
                        w,
                        h,
                    );
                } else {
                    gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
                }
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::RENDERBUFFER,
                    Some(color),
                );
                let depth = gl.create_renderbuffer()?;
                gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
                if samples > 0 {
                    gl.renderbuffer_storage_multisample(
                        glow::RENDERBUFFER,
                        samples,
                        glow::DEPTH_COMPONENT24,
                        w,
                        h,
                    );
                } else {
                    gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, w, h);
                }
                gl.framebuffer_renderbuffer(
                    glow::FRAMEBUFFER,
                    glow::DEPTH_ATTACHMENT,
                    glow::RENDERBUFFER,
                    Some(depth),
                );
                gl.bind_renderbuffer(glow::RENDERBUFFER, None);
                let ok =
                    gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE;
                while gl.get_error() != glow::NO_ERROR {}
                let mut resolve = None;
                let mut ok = ok;
                if ok && samples > 0 {
                    let rfbo = gl.create_framebuffer()?;
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(rfbo));
                    let rcol = gl.create_renderbuffer()?;
                    gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rcol));
                    gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, w, h);
                    gl.framebuffer_renderbuffer(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::RENDERBUFFER,
                        Some(rcol),
                    );
                    gl.bind_renderbuffer(glow::RENDERBUFFER, None);
                    if gl.check_framebuffer_status(glow::FRAMEBUFFER) == glow::FRAMEBUFFER_COMPLETE
                    {
                        resolve = Some((rfbo, rcol));
                    } else {
                        gl.delete_framebuffer(rfbo);
                        gl.delete_renderbuffer(rcol);
                        ok = false;
                    }
                    while gl.get_error() != glow::NO_ERROR {}
                }
                if ok {
                    self.samples = samples;
                    self.target = Some(Target {
                        fbo,
                        color,
                        depth,
                        resolve,
                        w,
                        h,
                    });
                    return Ok(());
                }
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(fbo);
                gl.delete_renderbuffer(color);
                gl.delete_renderbuffer(depth);
            }
        }
        Err("çerçeve tamponu oluşturulamadı".into())
    }

    fn free_target(&mut self, gl: &glow::Context) {
        if let Some(t) = self.target.take() {
            unsafe {
                gl.delete_framebuffer(t.fbo);
                gl.delete_renderbuffer(t.color);
                gl.delete_renderbuffer(t.depth);
                if let Some((f, c)) = t.resolve {
                    gl.delete_framebuffer(f);
                    gl.delete_renderbuffer(c);
                }
            }
        }
    }

    /// Çizim MSAA ile başarısız olduysa örneklemesiz yola geçer. Geçilebildiyse `true`
    /// (o zaman bir sonraki kare yeniden denenir; değilse 2D'ye düşülür).
    pub fn disable_msaa(&mut self, gl: &glow::Context) -> bool {
        if self.samples == 0 {
            return false;
        }
        self.samples = 0;
        self.free_target(gl);
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            while gl.get_error() != glow::NO_ERROR {}
        }
        true
    }

    /// Bir kareyi çizer. `info` egui'nin geri çağrı bilgisidir.
    pub fn paint(
        &mut self,
        gl: &glow::Context,
        info: &egui::PaintCallbackInfo,
        out_fbo: Option<glow::Framebuffer>,
        f: &FrameData,
    ) -> Result<(), String> {
        let vp = info.viewport_in_pixels();
        let (w, h) = (vp.width_px, vp.height_px);
        if w <= 0 || h <= 0 {
            return Ok(());
        }
        if self.scene_version != f.scene_version {
            self.upload_scene(gl, &f.scene)?;
            self.scene_version = f.scene_version;
        }
        if self.grid_version != f.grid.version {
            self.upload_grid(gl, &f.grid)?;
            self.grid_version = f.grid.version;
        }
        self.ensure_target(gl, w, h)?;
        let Some(target) = &self.target else {
            return Err("hedef yok".into());
        };
        unsafe {
            // egui'den kalan bayat GL hatası bu karede yanlış alarm vermesin.
            while gl.get_error() != glow::NO_ERROR {}
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
            gl.viewport(0, 0, w, h);
            gl.disable(glow::SCISSOR_TEST);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.clear_color(f.clear[0], f.clear[1], f.clear[2], 1.0);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);

            gl.use_program(Some(self.program));
            gl.uniform_matrix_4_f32_slice(self.u_vp.as_ref(), false, &f.view_proj);
            gl.uniform_3_f32(
                self.u_light.as_ref(),
                f.light_dir[0],
                f.light_dir[1],
                f.light_dir[2],
            );
            gl.uniform_3_f32(self.u_eye.as_ref(), f.eye[0], f.eye[1], f.eye[2]);

            let color_of = |k: PartKind| f.colors.iter().find(|(c, _)| *c == k).map(|(_, c)| *c);

            // 1) Opak parçalar
            gl.uniform_1_f32(self.u_lit.as_ref(), 1.0);
            for m in self.meshes.iter().filter(|m| !m.kind.translucent()) {
                let Some(c) = color_of(m.kind) else { continue };
                gl.uniform_4_f32(self.u_color.as_ref(), c[0], c[1], c[2], 1.0);
                gl.bind_vertex_array(Some(m.vao));
                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
            }
            // 2) Zemin ızgarası (aydınlatmasız çizgiler)
            if let Some(l) = &self.lines {
                gl.enable(glow::BLEND);
                gl.blend_func_separate(
                    glow::SRC_ALPHA,
                    glow::ONE_MINUS_SRC_ALPHA,
                    glow::ONE,
                    glow::ONE_MINUS_SRC_ALPHA,
                );
                gl.depth_mask(false);
                gl.uniform_1_f32(self.u_lit.as_ref(), 0.0);
                gl.bind_vertex_array(Some(l.vao));
                let c = f.grid_minor;
                gl.uniform_4_f32(self.u_color.as_ref(), c[0], c[1], c[2], c[3]);
                if l.minor > 0 {
                    gl.draw_arrays(glow::LINES, 0, l.minor);
                }
                let c = f.grid_major;
                gl.uniform_4_f32(self.u_color.as_ref(), c[0], c[1], c[2], c[3]);
                if l.major > 0 {
                    gl.draw_arrays(glow::LINES, l.minor, l.major);
                }
            }
            // 3) Yarı saydam pervane diskleri (derinliğe yazmadan)
            gl.enable(glow::BLEND);
            gl.blend_func_separate(
                glow::SRC_ALPHA,
                glow::ONE_MINUS_SRC_ALPHA,
                glow::ONE,
                glow::ONE_MINUS_SRC_ALPHA,
            );
            gl.depth_mask(false);
            gl.uniform_1_f32(self.u_lit.as_ref(), 1.0);
            for m in self.meshes.iter().filter(|m| m.kind.translucent()) {
                let Some(c) = color_of(m.kind) else { continue };
                gl.uniform_4_f32(self.u_color.as_ref(), c[0], c[1], c[2], c[3]);
                gl.bind_vertex_array(Some(m.vao));
                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
            }
            gl.bind_vertex_array(None);
            gl.depth_mask(true);
            gl.disable(glow::DEPTH_TEST);

            // 4) MSAA ise önce örneklemesiz ara tampona çöz.
            let mut src = target.fbo;
            if let Some((rfbo, _)) = target.resolve {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(target.fbo));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(rfbo));
                gl.blit_framebuffer(
                    0,
                    0,
                    w,
                    h,
                    0,
                    0,
                    w,
                    h,
                    glow::COLOR_BUFFER_BIT,
                    glow::NEAREST,
                );
                src = rfbo;
            }
            // 5) Görünüm dikdörtgenine kopyala (makas egui'nin kırpma alanına göre).
            let clip = info.clip_rect_in_pixels();
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(src));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, out_fbo);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            );
            gl.blit_framebuffer(
                0,
                0,
                w,
                h,
                vp.left_px,
                vp.from_bottom_px,
                vp.left_px + w,
                vp.from_bottom_px + h,
                glow::COLOR_BUFFER_BIT,
                glow::NEAREST,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, out_fbo);
            gl.use_program(None);
            let err = gl.get_error();
            if err != glow::NO_ERROR {
                while gl.get_error() != glow::NO_ERROR {}
                return Err(format!("GL hata kodu {err:#x}"));
            }
        }
        Ok(())
    }
}

/// Bir renk (0..255) → GL rengi (0..1).
pub fn rgba(c: egui::Color32, alpha: f32) -> [f32; 4] {
    [
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        alpha.clamp(0.0, 1.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_packing_roundtrips() {
        let b = f32_bytes(&[1.0, -2.5]);
        assert_eq!(b.len(), 8);
        assert_eq!(f32::from_ne_bytes([b[4], b[5], b[6], b[7]]), -2.5);
        assert_eq!(u32_bytes(&[7, 9]).len(), 8);
        let c = rgba(egui::Color32::from_rgb(255, 0, 51), 2.0);
        assert_eq!(c, [1.0, 0.0, 0.2, 1.0]);
    }
}
