//! The sprite editor: in a window on macOS, or in a web page served by `sprite-server`.

#[cfg(not(target_arch = "wasm32"))]
mod native;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::main()
}
