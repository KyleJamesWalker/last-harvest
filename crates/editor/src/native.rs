//! The editor in a window on macOS.

use std::error::Error;
use std::num::NonZeroU32;
use std::time::Instant;

use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use last_harvest_render::{Profile, Renderer};
use raw_window_handle::HasWindowHandle;
use sprite_editor::editor::Key;
use sprite_editor::sprite;
use sprite_editor::store::FsStore;
use sprite_editor::studio::Studio;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

const WINDOW: [f32; 2] = [1500.0, 750.0];

struct Gl {
    window: Window,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    gl: glow::Context,
    renderer: Renderer,
}

struct App {
    gl: Option<Gl>,
    studio: Studio,
    cursor: [f32; 2],
    modifiers: ModifiersState,
    last: Instant,
    frames: u32,
    i_held: bool,
    /// Saves the window as a PNG at the end of the next frame.
    shot: Option<std::path::PathBuf>,
}

impl App {
    fn new() -> Self {
        let store = FsStore::default();
        eprintln!("sprites: {}", store.dir.display());
        Self {
            gl: None,
            studio: Studio::new(Box::new(store), &std::env::var("SPRITE_EDITOR_OPEN").unwrap_or_else(|_| "SCARECROW".into())),
            cursor: [0.0; 2],
            modifiers: ModifiersState::empty(),
            last: Instant::now(),
            frames: 0,
            i_held: false,
            shot: None,
        }
    }

    fn frame(&mut self) {
        let dt = self.last.elapsed().as_secs_f32().min(0.05);
        self.last = Instant::now();
        let Some(g) = &mut self.gl else { return };
        if let Some(pixels) = self.studio.tick(dt) {
            g.renderer.set_art(&g.gl, pixels);
        }
        g.window.set_title(&self.studio.editor.title());
        let s = g.window.scale_factor() as f32;
        let px = |v: f32| (v * s).round() as i32;
        for (canvas, [x, y, w, h]) in self.studio.canvases().iter().zip(Studio::panes(WINDOW)) {
            g.renderer.draw(&g.gl, canvas, [px(x), px(WINDOW[1] - y - h), px(w), px(h)]);
        }
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
        self.studio.editor.nudge = if self.modifiers.shift_key() { 8 } else { 1 };
        self.studio.editor.key(k);
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
                self.studio.editor.changed = true;
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
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                self.studio.editor.peek(self.i_held || self.modifiers.alt_key());
            }
            // I and Option hold the color picker; letting go returns to the tool.
            WindowEvent::KeyboardInput { event, .. } if matches!(&event.logical_key, WKey::Character(c) if c.eq_ignore_ascii_case("i")) && !self.studio.editor.wants_text() => {
                if !event.repeat {
                    self.i_held = event.state == ElementState::Pressed;
                    self.studio.editor.peek(self.i_held || self.modifiers.alt_key());
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => self.key(&event.logical_key),
            WindowEvent::CursorMoved { position, .. } => {
                let s = self.gl.as_ref().map_or(1.0, |g| g.window.scale_factor()) as f32;
                let now = [position.x as f32 / s, position.y as f32 / s];
                let (pane, p) = Studio::locate(WINDOW, now);
                let (_, before) = Studio::locate(WINDOW, self.cursor);
                self.cursor = now;
                self.studio.motion(pane, p, p[0] - before[0]);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (pane, p) = Studio::locate(WINDOW, self.cursor);
                match state {
                    ElementState::Pressed if matches!(button, MouseButton::Left | MouseButton::Right) => {
                        self.studio.press(pane, p, button == MouseButton::Right, self.modifiers.shift_key());
                    }
                    ElementState::Pressed => {}
                    ElementState::Released => self.studio.release(),
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
                };
                let (pane, p) = Studio::locate(WINDOW, self.cursor);
                self.studio.wheel(pane, p, dy);
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
    let attrs = Window::default_attributes().with_title("LastHarvest sprite editor").with_inner_size(LogicalSize::new(WINDOW[0], WINDOW[1])).with_resizable(false);
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

pub fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::new())?;
    Ok(())
}
