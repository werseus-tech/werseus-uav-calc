// SPDX-License-Identifier: AGPL-3.0-or-later
//! eframe/glow üzerinden küçük mesh çizici (`egui::PaintCallback` içinde çalışır).
//!
//! Tasarım:
//! - Kendi çok örneklemeli (MSAA) çerçeve tamponuna çizer, sonra görünüm dikdörtgenine kopyalar
//!   (blit). Böylece ana pencerenin derinlik tamponuna ve MSAA ayarına bağımlı değildir; kenarlar
//!   yumuşak görünür.
//! - Tek gölgelendirici: yarım küre ortam ışığı + ana ve dolgu yönlü ışıklar + parça başına
//!   Blinn-Phong parlama, iki yüzlü aydınlatma (pervane diskleri ve ince yüzeyler arkadan da
//!   görünür). Aynı gölgelendirici zeminde yumuşak bir temas gölgesi (elips lekesi) de çizer.
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
    /// Ana ışık yönü (yüzeyden ışığa, birim).
    pub light_dir: [f32; 3],
    /// Dolgu ışığı yönü (ana ışığın karşı tarafı, daha sönük).
    pub fill_dir: [f32; 3],
    /// Yarım küre ortam ışığı: gökyüzü (yukarı bakan) ve zemin (aşağı bakan) rengi.
    pub sky: [f32; 3],
    pub ground: [f32; 3],
    /// Tür → (parlama şiddeti, parlaklık üssü).
    pub materials: Vec<(PartKind, [f32; 2])>,
    /// Patlatılmış görünümde tür başına kayma.
    pub offsets: Vec<(PartKind, [f32; 3])>,
    /// Zemin temas gölgesi: (merkez x, merkez y, yarı boy x, yarı boy y), zemin z'si, koyuluk.
    pub shadow: [f32; 4],
    pub ground_z: f32,
    pub shadow_alpha: f32,
    pub clear: [f32; 4],
    pub grid_minor: [f32; 4],
    pub grid_major: [f32; 4],
    /// Tür → renk (RGBA, 0..1). Listede olmayan tür çizilmez.
    pub colors: Vec<(PartKind, [f32; 4])>,
    pub scene: Arc<Scene>,
    pub scene_version: u64,
    /// Zemin ızgarası çizgileri (x,y,z çiftleri) ve ana çizgi ayrımı.
    pub grid: Arc<GridData>,
    /// İsteğe bağlı dünya ortamı ağı (uçuş simülatörü arazi/pist/şehir).
    pub world: Option<Arc<crate::sim::terrain::WorldMesh>>,
    pub world_vp: [f32; 16],
    pub world_eye: [f32; 3],
    pub world_light: [f32; 3],
    pub world_fill: [f32; 3],
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

struct GpuWorld {
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    ibo: glow::Buffer,
    count: i32,
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
    u_fill: Option<glow::UniformLocation>,
    u_sky: Option<glow::UniformLocation>,
    u_gcol: Option<glow::UniformLocation>,
    u_mat: Option<glow::UniformLocation>,
    u_mode: Option<glow::UniformLocation>,
    u_shadow: Option<glow::UniformLocation>,
    u_ground: Option<glow::UniformLocation>,
    u_off: Option<glow::UniformLocation>,
    shadow_quad: Option<(glow::VertexArray, glow::Buffer)>,
    a_pos: u32,
    a_nrm: Option<u32>,
    a_col: Option<u32>,
    meshes: Vec<GpuMesh>,
    scene_version: u64,
    lines: Option<GpuLines>,
    grid_version: u64,
    world: Option<GpuWorld>,
    world_version: u64,
    target: Option<Target>,
    samples: i32,
}

const VS: &str = r#"
precision highp float;
in vec3 a_pos;
in vec3 a_nrm;
in vec4 a_col;
uniform mat4 u_vp;
uniform float u_mode;
uniform vec4 u_shadow;
uniform float u_ground;
uniform vec3 u_off;
out vec3 v_nrm;
out vec3 v_pos;
out vec2 v_uv;
out vec4 v_col;
void main() {
    vec3 p = a_pos + u_off;
    if (u_mode > 1.5) {
        // Zemin lekesi: birim kare (-1..1) → elips kutusu, zemin düzleminde.
        p = vec3(u_shadow.xy + a_pos.xy * u_shadow.zw, u_ground);
    }
    v_nrm = a_nrm;
    v_pos = p;
    v_uv = a_pos.xy;
    v_col = a_col;
    gl_Position = u_vp * vec4(p, 1.0);
}
"#;

// u_mode: 0 = aydınlatmasız (çizgiler), 1 = aydınlatmalı parça, 2 = zemin gölgesi.
const FS: &str = r#"
precision highp float;
in vec3 v_nrm;
in vec3 v_pos;
in vec2 v_uv;
in vec4 v_col;
uniform vec4 u_color;
uniform vec3 u_light;
uniform vec3 u_fill;
uniform vec3 u_eye;
uniform vec3 u_sky;
uniform vec3 u_gcol;
uniform vec2 u_mat;
uniform float u_mode;
out vec4 o_color;
void main() {
    if (u_mode > 1.5) {
        // Süper elips uzaklığı: uçak izine yakın, kenarları yumuşak leke.
        vec2 q = abs(v_uv);
        float d = pow(pow(q.x, 3.0) + pow(q.y, 3.0), 1.0 / 3.0);
        float a = 1.0 - smoothstep(0.35, 1.0, d);
        o_color = vec4(0.0, 0.0, 0.0, u_color.a * a * a);
        return;
    }
    vec3 base = u_color.rgb * v_col.rgb;
    vec3 c = base;
    if (u_mode > 0.5) {
        vec3 n = normalize(v_nrm);
        vec3 v = normalize(u_eye - v_pos);
        if (dot(n, v) < 0.0) { n = -n; }
        vec3 amb = mix(u_gcol, u_sky, 0.5 + 0.5 * n.z);
        float kd = max(dot(n, u_light), 0.0);
        float fd = max(dot(n, u_fill), 0.0);
        vec3 diff = amb * 0.60 + vec3(1.0, 0.97, 0.92) * (0.52 * kd) + vec3(0.80, 0.88, 1.0) * (0.20 * fd);
        float s1 = pow(max(dot(n, normalize(u_light + v)), 0.0), u_mat.y);
        float s2 = pow(max(dot(n, normalize(u_fill + v)), 0.0), u_mat.y);
        // Parlama yalnız ışığa bakan yüzde (kd > 0): arkadaki yansıma sızmasın.
        float spec = u_mat.x * (s1 * step(0.001, kd) + 0.35 * s2 * step(0.001, fd));
        float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.10;
        c = base * diff + vec3(spec + rim);
    }
    o_color = vec4(c, u_color.a * v_col.a);
}
"#;

/// Birim kare (-1..1, z=0) iki üçgen; zemin lekesi için.
fn make_quad(
    gl: &glow::Context,
    a_pos: u32,
    a_nrm: Option<u32>,
    a_col: Option<u32>,
) -> Result<(glow::VertexArray, glow::Buffer), String> {
    let v: [f32; 18] = [
        -1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0,
        0.0,
    ];
    unsafe {
        let vao = gl.create_vertex_array()?;
        gl.bind_vertex_array(Some(vao));
        let vbo = gl.create_buffer()?;
        gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &f32_bytes(&v), glow::STATIC_DRAW);
        gl.enable_vertex_attrib_array(a_pos);
        gl.vertex_attrib_pointer_f32(a_pos, 3, glow::FLOAT, false, 12, 0);
        if let Some(n) = a_nrm {
            gl.disable_vertex_attrib_array(n);
            gl.vertex_attrib_3_f32(n, 0.0, 0.0, 1.0);
        }
        if let Some(c) = a_col {
            gl.disable_vertex_attrib_array(c);
            gl.vertex_attrib_4_f32(c, 1.0, 1.0, 1.0, 1.0);
        }
        gl.bind_vertex_array(None);
        Ok((vao, vbo))
    }
}

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
            let a_col = gl.get_attrib_location(program, "a_col");
            let max_samples = gl.get_parameter_i32(glow::MAX_SAMPLES);
            // Bazı sürücüler hata kodu bırakır; temizle.
            while gl.get_error() != glow::NO_ERROR {}
            Ok(Self {
                u_vp: gl.get_uniform_location(program, "u_vp"),
                u_color: gl.get_uniform_location(program, "u_color"),
                u_light: gl.get_uniform_location(program, "u_light"),
                u_eye: gl.get_uniform_location(program, "u_eye"),
                u_fill: gl.get_uniform_location(program, "u_fill"),
                u_sky: gl.get_uniform_location(program, "u_sky"),
                u_gcol: gl.get_uniform_location(program, "u_gcol"),
                u_mat: gl.get_uniform_location(program, "u_mat"),
                u_mode: gl.get_uniform_location(program, "u_mode"),
                u_shadow: gl.get_uniform_location(program, "u_shadow"),
                u_ground: gl.get_uniform_location(program, "u_ground"),
                u_off: gl.get_uniform_location(program, "u_off"),
                shadow_quad: None,
                program,
                a_pos,
                a_nrm,
                a_col,
                meshes: Vec::new(),
                scene_version: u64::MAX,
                lines: None,
                grid_version: u64::MAX,
                world: None,
                world_version: u64::MAX,
                target: None,
                samples: max_samples.clamp(0, 4),
            })
        }
    }

    unsafe fn bind_attribs(
        &self,
        gl: &glow::Context,
        stride: i32,
        with_normal: bool,
        with_color: bool,
    ) {
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
            if let Some(c) = self.a_col {
                if with_color {
                    gl.enable_vertex_attrib_array(c);
                    gl.vertex_attrib_pointer_f32(c, 4, glow::FLOAT, false, stride, 24);
                } else {
                    gl.disable_vertex_attrib_array(c);
                    gl.vertex_attrib_4_f32(c, 1.0, 1.0, 1.0, 1.0);
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
                self.bind_attribs(gl, 24, true, false);
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
            self.bind_attribs(gl, 12, false, false);
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

    fn upload_world(
        &mut self,
        gl: &glow::Context,
        w: Option<&crate::sim::terrain::WorldMesh>,
    ) -> Result<(), String> {
        unsafe {
            if let Some(old) = self.world.take() {
                gl.delete_vertex_array(old.vao);
                gl.delete_buffer(old.vbo);
                gl.delete_buffer(old.ibo);
            }
            let Some(wm) = w else {
                return Ok(());
            };
            if wm.indices.is_empty() || !wm.is_valid() {
                return Ok(());
            }
            let mut inter = Vec::with_capacity(wm.positions.len() * 10);
            for ((p, n), c) in wm.positions.iter().zip(&wm.normals).zip(&wm.colors) {
                inter.extend_from_slice(p);
                inter.extend_from_slice(n);
                inter.extend_from_slice(c);
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
                &u32_bytes(&wm.indices),
                glow::STATIC_DRAW,
            );
            self.bind_attribs(gl, 40, true, true);
            gl.bind_vertex_array(None);
            self.world = Some(GpuWorld {
                vao,
                vbo,
                ibo,
                count: wm.indices.len() as i32,
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
        let target_world_ver = f.world.as_ref().map_or(u64::MAX - 1, |wm| wm.version);
        if self.world_version != target_world_ver {
            self.upload_world(gl, f.world.as_deref())?;
            self.world_version = target_world_ver;
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
            gl.uniform_3_f32(self.u_sky.as_ref(), f.sky[0], f.sky[1], f.sky[2]);
            gl.uniform_3_f32(self.u_gcol.as_ref(), f.ground[0], f.ground[1], f.ground[2]);
            if let Some(world) = &self.world {
                gl.uniform_matrix_4_f32_slice(self.u_vp.as_ref(), false, &f.world_vp);
                gl.uniform_3_f32(
                    self.u_light.as_ref(),
                    f.world_light[0],
                    f.world_light[1],
                    f.world_light[2],
                );
                gl.uniform_3_f32(
                    self.u_eye.as_ref(),
                    f.world_eye[0],
                    f.world_eye[1],
                    f.world_eye[2],
                );
                gl.uniform_3_f32(
                    self.u_fill.as_ref(),
                    f.world_fill[0],
                    f.world_fill[1],
                    f.world_fill[2],
                );
                gl.uniform_3_f32(self.u_off.as_ref(), 0.0, 0.0, 0.0);
                gl.uniform_2_f32(self.u_mat.as_ref(), 0.04, 16.0);
                gl.uniform_4_f32(self.u_color.as_ref(), 1.0, 1.0, 1.0, 1.0);
                gl.uniform_1_f32(self.u_mode.as_ref(), 1.0);
                gl.bind_vertex_array(Some(world.vao));
                gl.draw_elements(glow::TRIANGLES, world.count, glow::UNSIGNED_INT, 0);
            }
            gl.uniform_matrix_4_f32_slice(self.u_vp.as_ref(), false, &f.view_proj);
            gl.uniform_3_f32(
                self.u_light.as_ref(),
                f.light_dir[0],
                f.light_dir[1],
                f.light_dir[2],
            );
            gl.uniform_3_f32(self.u_eye.as_ref(), f.eye[0], f.eye[1], f.eye[2]);
            gl.uniform_3_f32(
                self.u_fill.as_ref(),
                f.fill_dir[0],
                f.fill_dir[1],
                f.fill_dir[2],
            );

            let color_of = |k: PartKind| f.colors.iter().find(|(c, _)| *c == k).map(|(_, c)| *c);
            let mat_of = |k: PartKind| {
                f.materials
                    .iter()
                    .find(|(c, _)| *c == k)
                    .map(|(_, m)| *m)
                    .unwrap_or([0.2, 32.0])
            };

            let off_of = |k: PartKind| {
                f.offsets
                    .iter()
                    .find(|(c, _)| *c == k)
                    .map(|(_, o)| *o)
                    .unwrap_or([0.0; 3])
            };
            // Alfası 1'den küçük renkler (Yapı biçiminde dış yüzey, pervane diski) son geçişte.
            let is_clear =
                |k: PartKind| k.translucent() || color_of(k).is_some_and(|c| c[3] < 0.999);
            // 1) Opak parçalar
            gl.uniform_1_f32(self.u_mode.as_ref(), 1.0);
            for m in self.meshes.iter().filter(|m| !is_clear(m.kind)) {
                let Some(c) = color_of(m.kind) else { continue };
                let o = off_of(m.kind);
                gl.uniform_3_f32(self.u_off.as_ref(), o[0], o[1], o[2]);
                let mt = mat_of(m.kind);
                gl.uniform_2_f32(self.u_mat.as_ref(), mt[0], mt[1]);
                gl.uniform_4_f32(self.u_color.as_ref(), c[0], c[1], c[2], c[3]);
                gl.bind_vertex_array(Some(m.vao));
                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
            }
            gl.uniform_3_f32(self.u_off.as_ref(), 0.0, 0.0, 0.0);
            // 1b) Zemin temas gölgesi (derinliğe yazmadan, ızgaranın altında)
            if f.shadow_alpha > 0.0 && f.shadow[2] > 0.0 && f.shadow[3] > 0.0 {
                if self.shadow_quad.is_none() {
                    self.shadow_quad = Some(make_quad(gl, self.a_pos, self.a_nrm, self.a_col)?);
                }
                if let Some((vao, _)) = &self.shadow_quad {
                    gl.enable(glow::BLEND);
                    gl.blend_func_separate(
                        glow::SRC_ALPHA,
                        glow::ONE_MINUS_SRC_ALPHA,
                        glow::ONE,
                        glow::ONE_MINUS_SRC_ALPHA,
                    );
                    gl.depth_mask(false);
                    gl.uniform_1_f32(self.u_mode.as_ref(), 2.0);
                    gl.uniform_4_f32(
                        self.u_shadow.as_ref(),
                        f.shadow[0],
                        f.shadow[1],
                        f.shadow[2],
                        f.shadow[3],
                    );
                    gl.uniform_1_f32(self.u_ground.as_ref(), f.ground_z);
                    gl.uniform_4_f32(self.u_color.as_ref(), 0.0, 0.0, 0.0, f.shadow_alpha);
                    gl.bind_vertex_array(Some(*vao));
                    gl.draw_arrays(glow::TRIANGLES, 0, 6);
                    gl.depth_mask(true);
                    gl.disable(glow::BLEND);
                }
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
                gl.uniform_1_f32(self.u_mode.as_ref(), 0.0);
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
            gl.uniform_1_f32(self.u_mode.as_ref(), 1.0);
            for m in self.meshes.iter().filter(|m| is_clear(m.kind)) {
                let Some(c) = color_of(m.kind) else { continue };
                let o = off_of(m.kind);
                gl.uniform_3_f32(self.u_off.as_ref(), o[0], o[1], o[2]);
                let mt = mat_of(m.kind);
                gl.uniform_2_f32(self.u_mat.as_ref(), mt[0], mt[1]);
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
