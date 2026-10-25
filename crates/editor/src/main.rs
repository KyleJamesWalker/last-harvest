//! The sprite editor: in a window on macOS, or in a web page served by `sprite-server`.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::main()
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::main();
}
