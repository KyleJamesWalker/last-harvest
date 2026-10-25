mod atlas;

use glow::HasContext;
use last_harvest_game::scene3d::{CardScene, MAX_FLASHES};
use last_harvest_game::{Canvas, SCREEN_H, SCREEN_W, Shape, art};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// OpenGL 3.3 core (macOS).
    Desktop,
    /// OpenGL ES 3.0 (Mali on the device).
    Gles,
}

const VERT: &str = r#"
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_local;
layout(location = 2) in vec4 a_color;
layout(location = 3) in float a_kind;
layout(location = 4) in vec2 a_uv;
uniform vec2 u_size;
out vec2 v_local;
out vec4 v_color;
out float v_kind;
out vec2 v_uv;
void main() {
    gl_Position = vec4(a_pos.x / u_size.x * 2.0 - 1.0, 1.0 - a_pos.y / u_size.y * 2.0, 0.0, 1.0);
    v_local = a_local;
    v_color = a_color;
    v_kind = a_kind;
    v_uv = a_uv;
}
"#;

const FRAG: &str = r#"
in vec2 v_local;
in vec4 v_color;
in float v_kind;
in vec2 v_uv;
uniform sampler2D u_atlas;
uniform sampler2D u_art;
out vec4 o_color;
void main() {
    if (v_kind > 2.5) {
        o_color = texture(u_art, v_uv) * v_color;
        return;
    }
    if (v_kind > 1.5) {
        o_color = texture(u_atlas, v_uv) * v_color;
        return;
    }
    float a = 1.0;
    if (v_kind > 0.5) {
        float d = length(v_local);
        a = 1.0 - smoothstep(1.0 - fwidth(d), 1.0, d);
    }
    o_color = vec4(v_color.rgb, v_color.a * a);
}
"#;

const CARD_VERT: &str = r#"
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec3 a_normal;
layout(location = 3) in vec4 a_tint;
uniform mat4 u_view_proj;
out vec3 v_world;
out vec2 v_uv;
out vec3 v_normal;
out vec4 v_tint;
void main() {
    gl_Position = u_view_proj * vec4(a_pos, 1.0);
    v_world = a_pos;
    v_uv = a_uv;
    v_normal = a_normal;
    v_tint = a_tint;
}
"#;

const CARD_FRAG: &str = r#"
in vec3 v_world;
in vec2 v_uv;
in vec3 v_normal;
in vec4 v_tint;
uniform sampler2D u_art;
uniform vec3 u_eye;
uniform vec3 u_light;
uniform vec3 u_light_color;
uniform float u_light_range;
uniform vec3 u_flash_pos[4];
uniform vec3 u_flash_color[4];
uniform float u_flash_range[4];
uniform vec3 u_ambient;
uniform float u_saturation;
uniform vec3 u_fog;
uniform vec2 u_fog_range;
out vec4 o_color;
void main() {
    vec4 t = texture(u_art, v_uv);
    if (t.a < 0.5) discard;
    vec3 base = t.rgb * v_tint.rgb;
    vec3 n = normalize(v_normal);
    vec3 to = u_light - v_world;
    float d = length(to);
    float reach = clamp(1.0 - d / u_light_range, 0.0, 1.0);
    float facing = 0.45 + 0.55 * abs(dot(n, to / max(d, 1e-3)));
    vec3 light = u_ambient + u_light_color * reach * reach * facing * 1.6;
    for (int i = 0; i < 4; i++) {
        vec3 f = u_flash_pos[i] - v_world;
        float fd = length(f);
        float k = clamp(1.0 - fd / max(u_flash_range[i], 1e-3), 0.0, 1.0);
        light += u_flash_color[i] * k * k * (0.45 + 0.55 * abs(dot(n, f / max(fd, 1e-3))));
    }
    vec3 lit = mix(base * light, base, v_tint.a);
    lit = mix(vec3(dot(lit, vec3(0.3, 0.59, 0.11))), lit, u_saturation);
    float fog = smoothstep(u_fog_range.x, u_fog_range.y, length(v_world - u_eye)) * (1.0 - 0.7 * v_tint.a);
    o_color = vec4(mix(lit, u_fog, fog), 1.0);
}
"#;

/// Position, uv, normal, then tint and glow.
const CARD_FLOATS: usize = 12;

const FLOATS_PER_VERT: usize = 11;
const KIND_RECT: f32 = 0.0;
const KIND_CIRCLE: f32 = 1.0;
const KIND_TEXTURE: f32 = 2.0;
const KIND_ART: f32 = 3.0;

pub struct Renderer {
    program: glow::Program,
    vao: glow::VertexArray,
    vbo: glow::Buffer,
    u_size: Option<glow::UniformLocation>,
    verts: Vec<f32>,
    atlas: atlas::Atlas,
    texture: glow::Texture,
    art: glow::Texture,
    card_program: glow::Program,
    card_vao: glow::VertexArray,
    card_vbo: glow::Buffer,
    card_verts: Vec<f32>,
    card_ibo: glow::Buffer,
    /// How many cards the index buffer covers.
    card_capacity: usize,
}

unsafe fn link(gl: &glow::Context, header: &str, vert: &str, frag: &str) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vert), (glow::FRAGMENT_SHADER, frag)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &format!("{header}{src}"));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
            shaders.push(shader);
        }
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }
        Ok(program)
    }
}

impl Renderer {
    pub fn new(gl: &glow::Context, profile: Profile) -> Result<Self, String> {
        let header = match profile {
            Profile::Desktop => "#version 330 core\n",
            Profile::Gles => "#version 300 es\nprecision mediump float;\n",
        };
        unsafe {
            let program = link(gl, header, VERT, FRAG)?;
            let card_program = link(gl, header, CARD_VERT, CARD_FRAG)?;
            let card_vao = gl.create_vertex_array()?;
            let card_vbo = gl.create_buffer()?;
            let card_ibo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(card_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(card_vbo));
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(card_ibo));
            for (loc, size, offset) in [(0, 3, 0), (1, 2, 3), (2, 3, 5), (3, 4, 8)] {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, size, glow::FLOAT, false, (CARD_FLOATS * 4) as i32, offset * 4);
            }

            let vao = gl.create_vertex_array()?;
            let vbo = gl.create_buffer()?;
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let stride = (FLOATS_PER_VERT * 4) as i32;
            for (loc, size, offset) in [(0, 2, 0), (1, 2, 2), (2, 4, 4), (3, 1, 8), (4, 2, 9)] {
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, size, glow::FLOAT, false, stride, offset * 4);
            }

            let atlas = atlas::Atlas::build()?;
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            let size = atlas::SIZE as i32;
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, size, size, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&atlas.rgba)));
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            let art = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(art));
            let (aw, ah) = (art::ATLAS_W as i32, art::ATLAS_H as i32);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, aw, ah, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(art::PIXELS)));
            for (param, value) in [(glow::TEXTURE_MIN_FILTER, glow::NEAREST), (glow::TEXTURE_MAG_FILTER, glow::NEAREST), (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE), (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE)] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, param, value as i32);
            }
            gl.use_program(Some(program));
            gl.uniform_1_i32(gl.get_uniform_location(program, "u_atlas").as_ref(), 0);
            gl.uniform_1_i32(gl.get_uniform_location(program, "u_art").as_ref(), 1);

            let u_size = gl.get_uniform_location(program, "u_size");
            Ok(Self { program, vao, vbo, u_size, verts: Vec::new(), atlas, texture, art, card_program, card_vao, card_vbo, card_verts: Vec::new(), card_ibo, card_capacity: 0 })
        }
    }

    /// Draws one screen's canvas into `viewport` (x, y, w, h in framebuffer pixels, GL origin).
    pub fn draw(&mut self, gl: &glow::Context, canvas: &Canvas, viewport: [i32; 4]) {
        self.verts.clear();
        for shape in &canvas.shapes {
            match *shape {
                Shape::Rect { x, y, w, h, color } => self.quad(x, y, w, h, color, KIND_RECT),
                Shape::Circle { x, y, r, color } => self.quad(x - r, y - r, 2.0 * r, 2.0 * r, color, KIND_CIRCLE),
                Shape::Art { art, x, y, scale, flip, angle, show, color } => {
                    let [ax, ay, aw, ah] = art.rect().map(f32::from);
                    let ah = ah * show;
                    let (sw, sh) = (art::ATLAS_W as f32, art::ATLAS_H as f32);
                    let (u0, u1) = (ax / sw, (ax + aw) / sw);
                    let uv = [if flip { u1 } else { u0 }, ay / sh, if flip { u0 } else { u1 }, (ay + ah) / sh];
                    let (hw, hh) = (aw * scale / 2.0, ah * scale / 2.0);
                    let (cx, cy, s, c) = (x + hw, y + hh, angle.sin(), angle.cos());
                    let corner = |dx: f32, dy: f32| [cx + dx * c - dy * s, cy + dx * s + dy * c];
                    self.push_quad([corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)], uv, color, KIND_ART);
                }
                Shape::Text { ref text, x, y, size, color, align } => {
                    for g in self.atlas.layout(text, x, y, size, align) {
                        let [gx, gy, gw, gh] = g.rect;
                        self.textured(gx, gy, gw, gh, g.uv, color);
                    }
                }
            }
        }
        let [x, y, w, h] = viewport;
        let [r, g, b, a] = canvas.clear;
        unsafe {
            gl.viewport(x, y, w, h);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(x, y, w, h);
            gl.clear_color(r, g, b, a);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::SCISSOR_TEST);
            if let Some(cards) = &canvas.cards {
                self.draw_cards(gl, cards);
            }

            gl.enable(glow::BLEND);
            gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
            gl.use_program(Some(self.program));
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.art));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            gl.uniform_2_f32(self.u_size.as_ref(), SCREEN_W, SCREEN_H);
            gl.bind_vertex_array(Some(self.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            let bytes = std::slice::from_raw_parts(self.verts.as_ptr().cast::<u8>(), self.verts.len() * 4);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            gl.draw_arrays(glow::TRIANGLES, 0, (self.verts.len() / FLOATS_PER_VERT) as i32);
        }
    }

    fn draw_cards(&mut self, gl: &glow::Context, scene: &CardScene) {
        self.card_verts.clear();
        for card in &scene.cards {
            let [u0, v0, u1, v1] = card.uv;
            let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
            let n = card.normal.to_array();
            let [r, g, b] = card.tint;
            for (corner, uv) in card.corners.iter().zip(uvs) {
                let p = corner.to_array();
                self.card_verts.extend_from_slice(&[p[0], p[1], p[2], uv[0], uv[1], n[0], n[1], n[2], r, g, b, card.glow]);
            }
        }
        let count = scene.cards.len();
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::BLEND);
            gl.use_program(Some(self.card_program));
            let p = self.card_program;
            let loc = |name| gl.get_uniform_location(p, name);
            gl.uniform_matrix_4_f32_slice(loc("u_view_proj").as_ref(), false, &scene.view_proj.to_cols_array());
            gl.uniform_3_f32_slice(loc("u_eye").as_ref(), &scene.eye.to_array());
            gl.uniform_3_f32_slice(loc("u_light").as_ref(), &scene.light.to_array());
            gl.uniform_3_f32_slice(loc("u_light_color").as_ref(), &scene.light_color);
            gl.uniform_1_f32(loc("u_light_range").as_ref(), scene.light_range);
            let mut pos = [0.0; 3 * MAX_FLASHES];
            let mut color = [0.0; 3 * MAX_FLASHES];
            let mut range = [0.0; MAX_FLASHES];
            for (i, f) in scene.flashes.iter().take(MAX_FLASHES).enumerate() {
                pos[i * 3..i * 3 + 3].copy_from_slice(&f.pos.to_array());
                color[i * 3..i * 3 + 3].copy_from_slice(&f.color);
                range[i] = f.range;
            }
            gl.uniform_3_f32_slice(loc("u_flash_pos").as_ref(), &pos);
            gl.uniform_3_f32_slice(loc("u_flash_color").as_ref(), &color);
            gl.uniform_1_f32_slice(loc("u_flash_range").as_ref(), &range);
            gl.uniform_3_f32_slice(loc("u_ambient").as_ref(), &scene.ambient);
            gl.uniform_1_f32(loc("u_saturation").as_ref(), scene.saturation);
            gl.uniform_3_f32_slice(loc("u_fog").as_ref(), &scene.fog);
            gl.uniform_2_f32_slice(loc("u_fog_range").as_ref(), &scene.fog_range);
            gl.uniform_1_i32(loc("u_art").as_ref(), 1);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.art));
            gl.bind_vertex_array(Some(self.card_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.card_vbo));
            let bytes = std::slice::from_raw_parts(self.card_verts.as_ptr().cast::<u8>(), self.card_verts.len() * 4);
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
            if count > self.card_capacity {
                self.card_capacity = count.next_power_of_two();
                let indices: Vec<u32> = (0..self.card_capacity as u32).flat_map(|k| [0, 1, 2, 0, 2, 3].map(|i| k * 4 + i)).collect();
                let bytes = std::slice::from_raw_parts(indices.as_ptr().cast::<u8>(), indices.len() * 4);
                gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(self.card_ibo));
                gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes, glow::STATIC_DRAW);
            }
            gl.draw_elements(glow::TRIANGLES, (count * 6) as i32, glow::UNSIGNED_INT, 0);
            gl.disable(glow::DEPTH_TEST);
        }
    }

    fn quad(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], kind: f32) {
        let corners = [[x, y], [x + w, y], [x + w, y + h], [x, y + h]];
        self.push_quad(corners, [0.0; 4], color, kind);
    }

    fn textured(&mut self, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], color: [f32; 4]) {
        self.push_quad([[x, y], [x + w, y], [x + w, y + h], [x, y + h]], uv, color, KIND_TEXTURE);
    }

    /// Corners run clockwise from top-left; `uv` is (u0, v0, u1, v1) for the same corners.
    fn push_quad(&mut self, corners: [[f32; 2]; 4], [u0, v0, u1, v1]: [f32; 4], [r, g, b, a]: [f32; 4], kind: f32) {
        let local = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
        let uvs = [[u0, v0], [u1, v0], [u1, v1], [u0, v1]];
        for i in [0, 1, 2, 0, 2, 3] {
            let ([px, py], [lx, ly], [u, v]) = (corners[i], local[i], uvs[i]);
            self.verts.extend_from_slice(&[px, py, lx, ly, r, g, b, a, kind, u, v]);
        }
    }
}
