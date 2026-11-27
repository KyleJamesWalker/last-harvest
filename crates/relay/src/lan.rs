//! Hosting on this network: the game's own relay (`Config::lan_host`) and a beacon every second to multicast group
//! 239.255.42.100 and the broadcast address on UDP port `BEACON_PORT`, so other games on the network list the hosts
//! heard in the last few seconds whose relay answers `/healthz`. Copied from supermariowar-rust's `lan_beacon`.

use crate::Config;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::sync::{Mutex, Once};
use std::time::{Duration, Instant};

/// The TCP port a game hosting on its own network serves its relay on.
pub const HOST_PORT: u16 = 12525;
pub const BEACON_PORT: u16 = 12526;
const BEACON_GROUP: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 100);
/// A beacon is `<MAGIC> <relay port>`; a new beacon format takes a new magic.
const MAGIC: &str = "LHLAN1";
/// A host leaves the list this long after its last beacon.
const FORGET_AFTER: Duration = Duration::from_secs(4);
/// A beacon whose relay did not answer is ignored this long.
const RECHECK_AFTER: Duration = Duration::from_secs(10);
/// Relays checked at once; beacons from further new hosts wait for a later beacon.
const MAX_CHECKING: usize = 4;
const CHECK_TIMEOUT: Duration = Duration::from_secs(1);

static DISCOVERY: Mutex<Discovery> = Mutex::new(Discovery {
    heard: BTreeMap::new(),
    checks: BTreeMap::new(),
});
static HOSTING: Mutex<bool> = Mutex::new(false);

enum Check {
    Checking,
    Answered,
    Failed(Instant),
}

/// The hosts heard from, by `address:port`, and whether their relays answered.
struct Discovery {
    /// When each host whose relay answered last beaconed.
    heard: BTreeMap<String, Instant>,
    checks: BTreeMap<String, Check>,
}

impl Discovery {
    /// Records a beacon; true when the caller must check the host's relay and report it with `checked`.
    fn beacon(&mut self, host: &str, now: Instant) -> bool {
        match self.checks.get(host) {
            Some(Check::Answered) => {
                self.heard.insert(host.to_string(), now);
                false
            }
            Some(Check::Checking) => false,
            Some(Check::Failed(at)) if now.duration_since(*at) < RECHECK_AFTER => false,
            _ => {
                if self
                    .checks
                    .values()
                    .filter(|c| matches!(c, Check::Checking))
                    .count()
                    >= MAX_CHECKING
                {
                    return false;
                }
                self.checks.insert(host.to_string(), Check::Checking);
                true
            }
        }
    }

    fn checked(&mut self, host: &str, answered: bool, now: Instant) {
        if answered {
            self.checks.insert(host.to_string(), Check::Answered);
            self.heard.insert(host.to_string(), now);
        } else {
            self.checks.insert(host.to_string(), Check::Failed(now));
        }
        self.forget_old(now);
    }

    /// Forgets hosts silent for `FORGET_AFTER`, so one that beacons again is checked again.
    fn forget_old(&mut self, now: Instant) {
        self.heard
            .retain(|_, at| now.duration_since(*at) < FORGET_AFTER);
        let heard = &self.heard;
        self.checks.retain(|host, check| match check {
            Check::Checking => true,
            Check::Answered => heard.contains_key(host),
            Check::Failed(at) => now.duration_since(*at) < RECHECK_AFTER,
        });
    }

    fn nearby(&mut self, now: Instant) -> Vec<String> {
        self.forget_old(now);
        self.heard.keys().cloned().collect()
    }
}

/// What a host beacons for its relay on `port`.
pub fn beacon_message(port: u16) -> String {
    format!("{MAGIC} {port}")
}

/// The relay port in a beacon, or none for anything else.
pub fn parse_beacon(text: &str) -> Option<u16> {
    let mut fields = text.split(' ');
    if fields.next() != Some(MAGIC) {
        return None;
    }
    let port = fields.next()?.parse().ok().filter(|&p| p != 0)?;
    fields.next().is_none().then_some(port)
}

/// Whether a relay at `address` answers `GET /healthz` with `ok`.
fn relay_answers(address: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, CHECK_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(CHECK_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CHECK_TIMEOUT));
    if stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: last-harvest\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut response = String::new();
    let _ = stream.take(1024).read_to_string(&mut response);
    response.starts_with("HTTP/1.1 200") && response.ends_with("\r\n\r\nok\n")
}

/// The address of the interface a route to a documentation address would leave by: this device on its network. No
/// packet is sent.
pub fn local_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip()).filter(|ip| !ip.is_unspecified())
}

/// Starts this game's relay on `port` for local-network clients and its beacon, once; later calls only report.
pub fn host(port: u16) -> Result<(), String> {
    let mut hosting = HOSTING.lock().unwrap_or_else(|e| e.into_inner());
    if !*hosting {
        crate::start(&Config::lan_host(port))?;
        broadcast(port);
        *hosting = true;
    }
    Ok(())
}

/// Announces the relay on `port` to the local network until the process ends.
fn broadcast(port: u16) {
    let message = beacon_message(port);
    let _ = std::thread::Builder::new()
        .name("lh-lan-beacon".into())
        .spawn(move || {
            let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
                return;
            };
            let _ = socket.set_broadcast(true);
            let _ = socket.set_multicast_ttl_v4(1);
            // Multicast also reaches listeners on this machine; some systems never loop a broadcast back.
            let targets = [BEACON_GROUP, Ipv4Addr::BROADCAST];
            loop {
                for target in &targets {
                    let _ = socket.send_to(message.as_bytes(), (*target, BEACON_PORT));
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
}

/// The beacon port, shared with other games on this device (several players may run on one machine).
fn shared_listener() -> Option<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).ok()?;
    socket.set_reuse_address(true).ok()?;
    #[cfg(unix)]
    socket.set_reuse_port(true).ok()?;
    socket
        .bind(&SocketAddr::from(([0, 0, 0, 0], BEACON_PORT)).into())
        .ok()?;
    let _ = socket.join_multicast_v4(&BEACON_GROUP, &Ipv4Addr::UNSPECIFIED);
    Some(socket.into())
}

/// Starts listening for beacons, once.
pub fn listen() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("lh-lan-listen".into())
            .spawn(|| {
                let Some(socket) = shared_listener() else {
                    return;
                };
                let mut buf = [0u8; 256];
                while let Ok((len, from)) = socket.recv_from(&mut buf) {
                    let Some(port) = std::str::from_utf8(&buf[..len]).ok().and_then(parse_beacon)
                    else {
                        continue;
                    };
                    let relay = SocketAddr::new(from.ip(), port);
                    let host = relay.to_string();
                    if !discovery().beacon(&host, Instant::now()) {
                        continue;
                    }
                    let _ = std::thread::Builder::new()
                        .name("lh-lan-check".into())
                        .spawn(move || {
                            let answered = relay_answers(relay);
                            discovery().checked(&host, answered, Instant::now());
                        });
                }
            });
    });
}

fn discovery() -> std::sync::MutexGuard<'static, Discovery> {
    DISCOVERY.lock().unwrap_or_else(|e| e.into_inner())
}

/// The hosts heard from recently whose relays answered, as `address:port`.
pub fn nearby() -> Vec<String> {
    discovery().nearby(Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn empty() -> Discovery {
        Discovery {
            heard: BTreeMap::new(),
            checks: BTreeMap::new(),
        }
    }

    #[test]
    fn a_beacon_reads_back_and_nothing_else_does() {
        assert_eq!(parse_beacon(&beacon_message(HOST_PORT)), Some(HOST_PORT));
        assert_eq!(parse_beacon("LHLAN1 8097"), Some(8097));
        for other in [
            "",
            "LHLAN1",
            "LHLAN1 0",
            "LHLAN1 70000",
            "LHLAN1 12525 x",
            "LHLAN2 12525",
            "SMWLAN1 1.0 12523",
            "lhlan1 12525",
        ] {
            assert_eq!(parse_beacon(other), None, "{other:?}");
        }
    }

    #[test]
    fn a_host_is_listed_only_once_its_relay_answers() {
        let mut d = empty();
        let now = Instant::now();
        assert!(d.beacon("10.0.0.5:12525", now));
        assert!(!d.beacon("10.0.0.5:12525", now), "one check at a time");
        assert!(d.nearby(now).is_empty());
        d.checked("10.0.0.5:12525", true, now);
        assert_eq!(d.nearby(now), ["10.0.0.5:12525"]);
        assert!(!d.beacon("10.0.0.5:12525", now + Duration::from_secs(3)));
        assert_eq!(
            d.nearby(now + Duration::from_secs(6)),
            ["10.0.0.5:12525"],
            "its beacons keep it listed"
        );
        assert!(
            d.nearby(now + Duration::from_secs(10)).is_empty(),
            "a host gone quiet expires"
        );
        assert!(
            d.beacon("10.0.0.5:12525", now + Duration::from_secs(10)),
            "a forgotten host is checked again"
        );
    }

    #[test]
    fn a_host_whose_relay_does_not_answer_is_not_listed_or_rechecked_at_once() {
        let mut d = empty();
        let now = Instant::now();
        assert!(d.beacon("10.0.0.6:12525", now));
        d.checked("10.0.0.6:12525", false, now);
        assert!(!d.beacon("10.0.0.6:12525", now + Duration::from_secs(1)));
        assert!(d.nearby(now + Duration::from_secs(1)).is_empty());
        assert!(d.beacon("10.0.0.6:12525", now + RECHECK_AFTER));
    }

    #[test]
    fn at_most_a_few_hosts_are_checked_at_once() {
        let mut d = empty();
        let now = Instant::now();
        let started = (0..10)
            .filter(|i| d.beacon(&format!("10.0.0.{i}:1"), now))
            .count();
        assert_eq!(started, MAX_CHECKING);
    }

    fn serve_once(response: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.read(&mut [0; 512]);
            let _ = stream.write_all(response.as_bytes());
        });
        address
    }

    #[test]
    fn only_a_relay_health_answer_counts() {
        assert!(relay_answers(serve_once(
            "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nok\n"
        )));
        assert!(!relay_answers(serve_once(
            "HTTP/1.1 404 Not Found\r\n\r\nnot found\n"
        )));
        assert!(!relay_answers(serve_once("LHLAN1\n")));
        let closed = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        assert!(!relay_answers(closed));
    }

    #[test]
    fn a_relay_started_here_answers_the_check() {
        let port = crate::start(&Config::lan_host(0)).unwrap();
        assert!(relay_answers(SocketAddr::from(([127, 0, 0, 1], port))));
    }
}
