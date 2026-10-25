//! A pixel editor for LastHarvest's sprites, with live previews in the game's 2.5D view and on its map.

mod editor;
mod sprite;

use std::error::Error;
use std::num::NonZeroU32;
use std::time::Instant;

use editor::{Editor, Key};
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use last_harvest_game::art::{self, ATLAS_W, PIXELS};
use last_harvest_game::{Align, Canvas, SCREEN_H, SCREEN_W, Showcase};
use last_harvest_render::{Profile, Renderer};
use raw_window_handle::HasWindowHandle;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

/// The editing pane on the left, and two previews stacked on the right, all 4:3 like the device's screens.
const EDIT_W: f32 = 1000.0;
const PREVIEW_W: f32 = 500.0;
const WINDOW_H: f32 = 750.0;
const SHOWCASE_SEED: u64 = 7;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    Edit,
    View,
    Map,
}

struct Gl {
    window: Window,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    gl: glow::Context,
    renderer: Renderer,
}

struct App {
    gl: Option<Gl>,
    editor: Editor,
    showcase: Showcase,
    /// The packed texture with the sprite being edited drawn into the scratch area.
    atlas: Vec<u8>,
    yaw: f32,
    distance: f32,
    zoom: f32,
    cursor: [f32; 2],
    held: Option<(Pane, MouseButton)>,
    modifiers: ModifiersState,
    last: Instant,
    frames: u32,
    /// Saves the window as a PNG at the end of the next frame.
    shot: Option<std::path::PathBuf>,
}

impl App {
    fn new() -> Self {
        let dir = sprite::folder();
        eprintln!("sprites: {}", dir.display());
        Self {
            gl: None,
            editor: Editor::new(dir, &std::env::var("SPRITE_EDITOR_OPEN").unwrap_or_else(|_| "SCARECROW".into())),
            showcase: Showcase::new(SHOWCASE_SEED),
            atlas: PIXELS.to_vec(),
            yaw: Showcase::yaw(),
            distance: 48.0,
            zoom: 2.0,
            cursor: [0.0; 2],
            held: None,
            modifiers: ModifiersState::empty(),
            last: Instant::now(),
            frames: 0,
            shot: None,
        }
    }

    /// Which pane a point in logical window pixels falls in, and where in that pane's 1024x768 canvas.
    fn locate(&self, [x, y]: [f32; 2]) -> (Pane, [f32; 2]) {
        if x < EDIT_W {
            (Pane::Edit, [x / EDIT_W * SCREEN_W, y / WINDOW_H * SCREEN_H])
        } else {
            let half = WINDOW_H / 2.0;
            let pane = if y < half { Pane::View } else { Pane::Map };
            (pane, [(x - EDIT_W) / PREVIEW_W * SCREEN_W, (y % half) / half * SCREEN_H])
        }
    }

    /// Draws the sprite's frames into the scratch area and points the live sprites at them.
    fn pack(&mut self) {
        let [sx, sy, sw, sh] = art::SCRATCH.rect().map(|v| v as usize);
        for row in sy..sy + sh {
            let i = (row * ATLAS_W as usize + sx) * 4;
            self.atlas[i..i + sw * 4].fill(0);
        }
        let mut rects = Vec::new();
        let (mut x, mut y, mut shelf) = (0, 0, 0);
        for f in self.editor.surfaces() {
            if x + f.w > sw {
                (x, y, shelf) = (0, y + shelf + 1, 0);
            }
            if f.w > sw || y + f.h > sh {
                break;
            }
            for row in 0..f.h {
                for col in 0..f.w {
                    let i = ((sy + y + row) * ATLAS_W as usize + sx + x + col) * 4;
                    self.atlas[i..i + 4].copy_from_slice(&f.px[row * f.w + col]);
                }
            }
            rects.push([(sx + x) as u16, (sy + y) as u16, f.w as u16, f.h as u16]);
            x += f.w + 1;
            shelf = shelf.max(f.h);
        }
        let live = art::set_live(rects, self.atlas.clone());
        self.editor.set_live(live);
        if let Some(g) = &mut self.gl {
            g.renderer.set_art(&g.gl, &self.atlas);
        }
    }

    fn frame(&mut self) {
        let dt = self.last.elapsed().as_secs_f32().min(0.05);
        self.last = Instant::now();
        self.editor.tick(dt);
        if self.editor.changed {
            self.pack();
            self.editor.changed = false;
        }
        self.showcase.show(self.editor.shown(), self.editor.stand(), self.editor.fps);
        self.showcase.update(dt, self.yaw, self.distance);

        let mut edit = Canvas::default();
        self.editor.draw(&mut edit);
        let mut view = self.showcase.top();
        view.label("2.5D view: drag to turn, scroll to move closer", 16.0, 12.0, 30.0, [1.0, 1.0, 1.0, 0.85], Align::Left);
        let mut map = self.showcase.map(self.zoom);
        map.label(format!("Map at {}x the game's scale: scroll to zoom", self.zoom), 16.0, 12.0, 30.0, [1.0, 1.0, 1.0, 0.85], Align::Left);

        let Some(g) = &mut self.gl else { return };
        g.window.set_title(&self.editor.title());
        let s = g.window.scale_factor() as f32;
        let px = |v: f32| (v * s).round() as i32;
        let half = WINDOW_H / 2.0;
        g.renderer.draw(&g.gl, &edit, [0, 0, px(EDIT_W), px(WINDOW_H)]);
        g.renderer.draw(&g.gl, &view, [px(EDIT_W), px(half), px(PREVIEW_W), px(half)]);
        g.renderer.draw(&g.gl, &map, [px(EDIT_W), 0, px(PREVIEW_W), px(half)]);
        self.frames += 1;
        // SPRITE_EDITOR_SHOT saves the window once the previews have settled, then quits: for checking the editor from scripts.
        let scripted = std::env::var_os("SPRITE_EDITOR_SHOT").filter(|_| self.frames == 40);
        if let Some(path) = self.shot.take().or(scripted.clone().map(Into::into)) {
            let size = g.window.inner_size();
            match save_shot(&g.gl, size.width, size.height, &path) {
                Ok(()) => eprintln!("saved {}", path.display()),
                Err(e) => eprintln!("shot failed: {e}"),
            }
            if scripted.is_some() {
                std::process::exit(0);
            }
        }
        if let Err(e) = g.surface.swap_buffers(&g.context) {
            eprintln!("swap: {e}");
        }
    }

    fn key(&mut self, key: &WKey) {
        let cmd = self.modifiers.super_key() || self.modifiers.control_key();
        let k = match key {
            WKey::Character(s) if cmd => match s.to_lowercase().as_str() {
                "z" if self.modifiers.shift_key() => Key::Redo,
                "z" => Key::Undo,
                "y" => Key::Redo,
                "s" => Key::Save,
                "c" => Key::Copy,
                "x" => Key::Cut,
                "v" => Key::Paste,
                _ => return,
            },
            WKey::Character(s) => match s.chars().next() {
                Some(c) => Key::Char(c.to_ascii_lowercase()),
                None => return,
            },
            WKey::Named(NamedKey::Space) => Key::Char(' '),
            WKey::Named(NamedKey::Enter) => Key::Enter,
            WKey::Named(NamedKey::Escape) => Key::Escape,
            WKey::Named(NamedKey::Backspace | NamedKey::Delete) => Key::Backspace,
            WKey::Named(NamedKey::ArrowUp) => Key::Up,
            WKey::Named(NamedKey::ArrowDown) => Key::Down,
            WKey::Named(NamedKey::ArrowLeft) => Key::Left,
            WKey::Named(NamedKey::ArrowRight) => Key::Right,
            WKey::Named(NamedKey::F12) => {
                self.shot = Some(sprite::folder().join(format!("editor-{}.png", self.frames)));
                return;
            }
            _ => return,
        };
        self.editor.nudge = if self.modifiers.shift_key() { 8 } else { 1 };
        self.editor.key(k);
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gl.is_some() {
            return;
        }
        match open(el) {
            Ok(g) => {
                self.gl = Some(g);
                self.editor.changed = true;
            }
            Err(e) => {
                eprintln!("gl: {e}");
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(g), Some(w), Some(h)) = (&self.gl, NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                    g.surface.resize(&g.context, w, h);
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => self.key(&event.logical_key),
            WindowEvent::CursorMoved { position, .. } => {
                let s = self.gl.as_ref().map_or(1.0, |g| g.window.scale_factor()) as f32;
                let now = [position.x as f32 / s, position.y as f32 / s];
                let moved = now[0] - self.cursor[0];
                self.cursor = now;
                match self.held {
                    Some((Pane::View, _)) => self.yaw += moved * 0.01,
                    _ => {
                        let (pane, p) = self.locate(now);
                        if pane == Pane::Edit {
                            self.editor.drag(p);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (pane, p) = self.locate(self.cursor);
                match state {
                    ElementState::Pressed => {
                        self.held = Some((pane, button));
                        if pane == Pane::Edit && matches!(button, MouseButton::Left | MouseButton::Right) {
                            self.editor.mouse_down(p, button == MouseButton::Right, self.modifiers.shift_key());
                        }
                    }
                    ElementState::Released => {
                        self.held = None;
                        self.editor.mouse_up();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
                };
                let (pane, p) = self.locate(self.cursor);
                match pane {
                    Pane::Edit => self.editor.wheel(p, dy),
                    Pane::View => self.distance = (self.distance - dy * 6.0).clamp(24.0, 200.0),
                    Pane::Map => self.zoom = (self.zoom + dy.signum()).clamp(1.0, 6.0),
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(g) = &self.gl {
            g.window.request_redraw();
        }
    }
}

fn open(el: &ActiveEventLoop) -> Result<Gl, Box<dyn Error>> {
    let attrs = Window::default_attributes().with_title("LastHarvest sprite editor").with_inner_size(LogicalSize::new(EDIT_W + PREVIEW_W, WINDOW_H)).with_resizable(false);
    let template = ConfigTemplateBuilder::new().with_depth_size(24);
    let (window, config) = DisplayBuilder::new()
        .with_window_attributes(Some(attrs))
        .build(el, template, |configs| configs.max_by_key(|c| (c.depth_size().min(24), c.num_samples().min(4))).expect("no GL config"))?;
    let window = window.ok_or("no window")?;
    let context_attrs = ContextAttributesBuilder::new()
        .with_profile(GlProfile::Core)
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(window.window_handle()?.as_raw()));
    let display = config.display();
    let not_current = unsafe { display.create_context(&config, &context_attrs)? };
    let surface_attrs = window.build_surface_attributes(Default::default())?;
    let surface = unsafe { display.create_window_surface(&config, &surface_attrs)? };
    let context = not_current.make_current(&surface)?;
    surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::MIN)).ok();
    let gl = unsafe { glow::Context::from_loader_function_cstr(|s| display.get_proc_address(s)) };
    let renderer = Renderer::new(&gl, Profile::Desktop)?;
    Ok(Gl { window, surface, context, gl, renderer })
}

fn save_shot(gl: &glow::Context, w: u32, h: u32, path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    use glow::HasContext;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    unsafe { gl.read_pixels(0, 0, w as i32, h as i32, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut rgba))) };
    let rows: Vec<u8> = rgba.chunks_exact((w * 4) as usize).rev().flatten().copied().collect();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rows)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::new())?;
    Ok(())
}
