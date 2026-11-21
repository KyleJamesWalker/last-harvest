//! The relay's settings, from the command line and the environment.

use crate::limits::TrustedProxies;
use std::env;
use std::time::Duration;

const USAGE: &str = "usage: last-harvest-relay [--port <n>] [--healthcheck]

  --port         TCP port for signaling, /ice and /healthz (env LAST_HARVEST_RELAY_PORT, default 8080)
  --healthcheck  GET /healthz on 127.0.0.1:<port> and exit 0 if it answers 200

environment:
  LAST_HARVEST_RELAY_ALLOWED_ORIGINS  comma-separated page origins allowed to connect, or * for any; native games send none
                                      (default: http://localhost:8095)
  LAST_HARVEST_RELAY_MAX_CLIENTS      simultaneous signaling peers (default 128)
  LAST_HARVEST_RELAY_MAX_CONNECTIONS  simultaneous TCP connections of any kind, handshakes included (default 256)
  LAST_HARVEST_RELAY_MAX_PER_ADDRESS  simultaneous connections from one client address, 0 for no limit (default 16)
  LAST_HARVEST_RELAY_MAX_PEERS_PER_ADDRESS  simultaneous signaling peers from one client address, 0 for no limit (default 8)
  LAST_HARVEST_RELAY_LONELY_SECONDS   seconds a signaling peer may wait alone in its room (default 900)
  LAST_HARVEST_RELAY_CONNECTS_PER_MINUTE  new connections one client address may open per minute, 0 for no limit (default 60)
  LAST_HARVEST_RELAY_TRUSTED_PROXIES  peers whose X-Forwarded-For names the client: none, loopback or private (loopback and
                                      private networks, for a proxy on a Docker network) (default loopback)
  LAST_HARVEST_RELAY_TURN_URL         the TURN server signaling peers get, such as turn:relay.example.com:3478 (default: none)
  LAST_HARVEST_RELAY_TURN_SECRET      coturn's static-auth-secret, which the relay mints credentials from
  LAST_HARVEST_RELAY_TURN_SECONDS     how long a TURN credential lasts; peers get fresh ones while connected (default 1200)

Loopback clients are exempt from the per-address limits. An IPv6 client address counts by its /56.";

const DEFAULT_ORIGINS: &str = "http://localhost:8095";
const LONELY_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const TURN_LIFETIME: Duration = Duration::from_secs(20 * 60);

#[derive(Clone, Debug)]
pub struct Config {
    /// TCP port for signaling, `/ice` and `/healthz`.
    pub port: u16,
    pub healthcheck: bool,
    /// Normalized page origins browsers may connect from; `*` allows any.
    pub allowed_origins: Vec<String>,
    /// Simultaneous signaling peers.
    pub max_clients: usize,
    /// Simultaneous TCP connections of any kind; each holds a thread.
    pub max_connections: usize,
    /// Simultaneous connections from one client address; 0 for no limit.
    pub max_per_address: usize,
    /// Simultaneous signaling peers from one client address; 0 for no limit.
    pub max_peers_per_address: usize,
    /// How long a signaling peer may stay alone in its room before the relay closes it.
    pub lonely_timeout: Duration,
    /// New connections one client address may open per minute; 0 for no limit.
    pub connects_per_minute: u32,
    pub trusted_proxies: TrustedProxies,
    /// The TURN server signaling peers get; none without a secret too.
    pub turn_url: Option<String>,
    pub turn_secret: Option<String>,
    /// How long a TURN credential lasts; a peer gets a fresh one every quarter of it.
    pub turn_lifetime: Duration,
    /// Serve only loopback and local-network clients, so a forwarded port does not make an open relay.
    pub lan_only: bool,
    /// Signaling rooms open at once in this process; none for no limit.
    pub max_rooms: Option<usize>,
}

impl Config {
    /// The environment, then the arguments; the error is the usage text or names the bad value.
    pub fn from_args_and_env() -> Result<Config, String> {
        let var = |name: &str| env::var(name).ok().filter(|v| !v.is_empty());
        let mut config = Config {
            port: 8080,
            healthcheck: false,
            allowed_origins: parse_origins(&var("LAST_HARVEST_RELAY_ALLOWED_ORIGINS").unwrap_or_else(|| DEFAULT_ORIGINS.to_string())),
            max_clients: 128,
            max_connections: 256,
            max_per_address: 16,
            max_peers_per_address: 8,
            lonely_timeout: LONELY_TIMEOUT,
            connects_per_minute: 60,
            trusted_proxies: TrustedProxies::Loopback,
            turn_url: var("LAST_HARVEST_RELAY_TURN_URL"),
            turn_secret: var("LAST_HARVEST_RELAY_TURN_SECRET"),
            turn_lifetime: TURN_LIFETIME,
            lan_only: false,
            max_rooms: None,
        };
        if let Some(seconds) = var("LAST_HARVEST_RELAY_TURN_SECONDS") {
            let seconds: u64 = seconds.parse().ok().filter(|&s| s >= 60).ok_or_else(|| format!("LAST_HARVEST_RELAY_TURN_SECONDS: not a number of seconds, 60 or more: {seconds}"))?;
            config.turn_lifetime = Duration::from_secs(seconds);
        }
        if let Some(port) = var("LAST_HARVEST_RELAY_PORT") {
            config.port = port.parse().map_err(|_| format!("LAST_HARVEST_RELAY_PORT: not a port: {port}"))?;
        }
        if let Some(max) = var("LAST_HARVEST_RELAY_MAX_CLIENTS") {
            config.max_clients = max.parse().map_err(|_| format!("LAST_HARVEST_RELAY_MAX_CLIENTS: not a number: {max}"))?;
        }
        if let Some(max) = var("LAST_HARVEST_RELAY_MAX_CONNECTIONS") {
            config.max_connections = max.parse().map_err(|_| format!("LAST_HARVEST_RELAY_MAX_CONNECTIONS: not a number: {max}"))?;
        }
        if let Some(max) = var("LAST_HARVEST_RELAY_MAX_PER_ADDRESS") {
            config.max_per_address = max.parse().map_err(|_| format!("LAST_HARVEST_RELAY_MAX_PER_ADDRESS: not a number: {max}"))?;
        }
        if let Some(max) = var("LAST_HARVEST_RELAY_MAX_PEERS_PER_ADDRESS") {
            config.max_peers_per_address = max.parse().map_err(|_| format!("LAST_HARVEST_RELAY_MAX_PEERS_PER_ADDRESS: not a number: {max}"))?;
        }
        if let Some(seconds) = var("LAST_HARVEST_RELAY_LONELY_SECONDS") {
            let seconds = seconds.parse().map_err(|_| format!("LAST_HARVEST_RELAY_LONELY_SECONDS: not a number: {seconds}"))?;
            config.lonely_timeout = Duration::from_secs(seconds);
        }
        if let Some(rate) = var("LAST_HARVEST_RELAY_CONNECTS_PER_MINUTE") {
            config.connects_per_minute = rate.parse().map_err(|_| format!("LAST_HARVEST_RELAY_CONNECTS_PER_MINUTE: not a number: {rate}"))?;
        }
        if let Some(trusted) = var("LAST_HARVEST_RELAY_TRUSTED_PROXIES") {
            config.trusted_proxies = TrustedProxies::parse(&trusted)
                .ok_or_else(|| format!("LAST_HARVEST_RELAY_TRUSTED_PROXIES: not none, loopback or private: {trusted}"))?;
        }

        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--port" => {
                    let value = args.next().ok_or(USAGE)?;
                    config.port = value.parse().map_err(|_| format!("--port: not a port: {value}"))?;
                }
                "--healthcheck" => config.healthcheck = true,
                _ => return Err(USAGE.to_string()),
            }
        }
        Ok(config)
    }

    /// The settings of a relay a game or a test runs in-process: any origin, no per-address limits, no TURN.
    pub fn embedded(port: u16) -> Config {
        Config {
            port,
            healthcheck: false,
            allowed_origins: vec!["*".to_string()],
            max_clients: 8,
            max_connections: 32,
            max_per_address: 0,
            max_peers_per_address: 0,
            lonely_timeout: LONELY_TIMEOUT,
            connects_per_minute: 0,
            trusted_proxies: TrustedProxies::Loopback,
            turn_url: None,
            turn_secret: None,
            turn_lifetime: TURN_LIFETIME,
            lan_only: false,
            max_rooms: None,
        }
    }

    /// The relay a game runs to host on its own network: `embedded`, serving only local-network clients, one room.
    pub fn lan_host(port: u16) -> Config {
        Config { lan_only: true, max_rooms: Some(1), ..Config::embedded(port) }
    }

    /// Whether a request with this `Origin` header may proceed.
    pub fn origin_allowed(&self, origin: Option<&str>) -> bool {
        if self.allowed_origins.iter().any(|o| o == "*") {
            return true;
        }
        // Browsers always send Origin; a client without one is a native game, which no web page can impersonate.
        match origin {
            Some(origin) => self.allowed_origins.contains(&normalize_origin(origin)),
            None => true,
        }
    }
}

fn normalize_origin(origin: &str) -> String {
    origin.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn parse_origins(list: &str) -> Vec<String> {
    list.split(',').map(normalize_origin).filter(|o| !o.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        let c = Config { allowed_origins: parse_origins("https://game.example.com,http://localhost:8095"), ..Config::embedded(0) };
        assert!(c.origin_allowed(Some("https://game.example.com")));
        assert!(c.origin_allowed(Some("HTTPS://Game.Example.com/")));
        assert!(c.origin_allowed(Some("http://localhost:8095")));
        assert!(!c.origin_allowed(Some("http://localhost:8001")));
        assert!(!c.origin_allowed(Some("https://evil.example")));
        assert!(c.origin_allowed(None));

        let any = Config { allowed_origins: parse_origins(" * "), ..Config::embedded(0) };
        assert!(any.origin_allowed(None));
        assert!(any.origin_allowed(Some("https://evil.example")));
    }
}
