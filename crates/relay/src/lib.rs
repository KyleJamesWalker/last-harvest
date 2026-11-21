//! `last-harvest-relay`: matchbox signaling on `/signal/<room>` for `last-harvest-net` peers, with TURN credentials for
//! each peer in its room, and `/healthz`. See docs/online.md.

pub mod config;
pub mod lan;
mod limits;
mod signal;
mod turn;
mod websocket;

pub use config::Config;
pub use turn::turn_credentials;

/// True when a relay on `127.0.0.1:<port>` answers `/healthz` with 200.
pub fn healthcheck(port: u16) -> bool {
    websocket::healthcheck(port)
}

/// Serves `config` from background threads and returns the bound port: `Config::embedded(0)` picks a free one.
pub fn start(config: &Config) -> Result<u16, String> {
    websocket::serve(config).map_err(|e| format!("Could not listen on port {}: {}", config.port, e))
}

/// Serves `config` until the process ends.
pub fn run(config: &Config) -> Result<(), String> {
    let port = start(config)?;
    println!("Ready! Port {port}, allowed origins: {}", config.allowed_origins.join(", "));
    loop {
        std::thread::park();
    }
}
