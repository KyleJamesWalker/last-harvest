//! Per-address limits on the relay's connections: how many one address holds open, and how fast it opens new ones.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Addresses tracked before those with nothing open and a full allowance are forgotten.
const PRUNE_AT: usize = 4096;

/// Why an address may not open another connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    TooManyOpen,
    TooFast,
}

struct Entry {
    open: usize,
    /// New connections the address may still open now, refilled at `per_minute`.
    allowance: f64,
    at: Instant,
    logged_refusal: bool,
}

impl Entry {
    fn refill(&mut self, per_minute: u32, now: Instant) {
        let earned =
            now.saturating_duration_since(self.at).as_secs_f64() * per_minute as f64 / 60.0;
        self.allowance = (self.allowance + earned).min(per_minute as f64);
        self.at = now;
    }
}

/// The limits, shared by every connection thread; a limit of 0 is no limit.
pub struct AddressLimits {
    max_open: usize,
    per_minute: u32,
    addresses: Mutex<HashMap<IpAddr, Entry>>,
}

impl AddressLimits {
    pub fn new(max_open: usize, per_minute: u32) -> Arc<AddressLimits> {
        Arc::new(AddressLimits {
            max_open,
            per_minute,
            addresses: Mutex::default(),
        })
    }

    /// Counts a new connection from `address` until the slot drops. Loopback is exempt.
    pub fn open(self: &Arc<Self>, address: IpAddr, now: Instant) -> Result<AddressSlot, Refusal> {
        let address = key(address);
        if address.is_loopback() || (self.max_open == 0 && self.per_minute == 0) {
            return Ok(AddressSlot {
                limits: self.clone(),
                address: None,
            });
        }
        let mut addresses = self.addresses.lock().unwrap_or_else(|e| e.into_inner());
        if addresses.len() >= PRUNE_AT {
            let per_minute = self.per_minute;
            addresses.retain(|_, e| {
                e.refill(per_minute, now);
                e.open > 0 || e.allowance < per_minute as f64
            });
        }
        let full = self.per_minute as f64;
        let entry = addresses.entry(address).or_insert(Entry {
            open: 0,
            allowance: full,
            at: now,
            logged_refusal: false,
        });
        entry.refill(self.per_minute, now);
        let refusal = if self.max_open > 0 && entry.open >= self.max_open {
            Some(Refusal::TooManyOpen)
        } else if self.per_minute > 0 && entry.allowance < 1.0 {
            Some(Refusal::TooFast)
        } else {
            None
        };
        if let Some(refusal) = refusal {
            if !entry.logged_refusal {
                println!(
                    "[relay] refusing {address}: {}",
                    if refusal == Refusal::TooFast {
                        "too many new connections"
                    } else {
                        "too many open"
                    }
                );
            }
            entry.logged_refusal = true;
            return Err(refusal);
        }
        entry.logged_refusal = false;
        entry.open += 1;
        if self.per_minute > 0 {
            entry.allowance -= 1.0;
        }
        Ok(AddressSlot {
            limits: self.clone(),
            address: Some(address),
        })
    }
}

/// One open connection of an address, given back on drop.
pub struct AddressSlot {
    limits: Arc<AddressLimits>,
    address: Option<IpAddr>,
}

impl Drop for AddressSlot {
    fn drop(&mut self) {
        let Some(address) = self.address else { return };
        if let Some(entry) = self
            .limits
            .addresses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&address)
        {
            entry.open = entry.open.saturating_sub(1);
        }
    }
}

/// IPv4 as is; IPv6 by its /56, a common size for one customer's delegated prefix.
pub fn key(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3] & 0xff00, 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

/// The peers whose `X-Forwarded-For` the relay believes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustedProxies {
    None,
    Loopback,
    /// Loopback and private networks, such as a Docker network between Caddy and the relay.
    Private,
}

impl TrustedProxies {
    pub fn parse(value: &str) -> Option<TrustedProxies> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(TrustedProxies::None),
            "loopback" => Some(TrustedProxies::Loopback),
            "private" => Some(TrustedProxies::Private),
            _ => None,
        }
    }
}

/// Loopback, a private or link-local network, or carrier-grade NAT space (as Tailscale uses): an address on this network.
pub fn is_local(address: IpAddr) -> bool {
    match address {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_local(IpAddr::V4(v4)),
            None => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
        },
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || (v4.octets()[0] == 100 && v4.octets()[1] & 0xc0 == 64)
        }
    }
}

/// Whether a connection from `address` may be a reverse proxy in front of the relay.
pub fn may_proxy(address: IpAddr, trusted: TrustedProxies) -> bool {
    let address = match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(address, IpAddr::V4),
        v4 => v4,
    };
    let private = match address {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local(),
    };
    match trusted {
        TrustedProxies::None => false,
        TrustedProxies::Loopback => address.is_loopback(),
        TrustedProxies::Private => address.is_loopback() || private,
    }
}

/// The value of the last `name` header in an HTTP request head.
pub fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines()
        .filter_map(|l| l.split_once(':'))
        .rfind(|(n, _)| n.trim().eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}

/// The client address a proxy put last in `X-Forwarded-For`, if `head` has one.
pub fn forwarded_for(head: &str) -> Option<IpAddr> {
    header(head, "x-forwarded-for")
        .and_then(|value| value.rsplit(',').next())
        .and_then(|last| last.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn an_address_holds_at_most_max_open() {
        let limits = AddressLimits::new(2, 0);
        let now = Instant::now();
        let a = limits.open(ip("203.0.113.1"), now).unwrap();
        let _b = limits.open(ip("203.0.113.1"), now).unwrap();
        assert_eq!(
            limits.open(ip("203.0.113.1"), now).err(),
            Some(Refusal::TooManyOpen)
        );
        assert!(
            limits.open(ip("203.0.113.2"), now).is_ok(),
            "other addresses are not affected"
        );
        drop(a);
        assert!(limits.open(ip("203.0.113.1"), now).is_ok());
    }

    #[test]
    fn new_connections_are_rate_limited_per_minute() {
        let limits = AddressLimits::new(0, 6);
        let now = Instant::now();
        for _ in 0..6 {
            drop(limits.open(ip("203.0.113.1"), now).unwrap());
        }
        assert_eq!(
            limits.open(ip("203.0.113.1"), now).err(),
            Some(Refusal::TooFast)
        );
        assert!(
            limits
                .open(ip("203.0.113.1"), now + Duration::from_secs(5))
                .is_err()
        );
        assert!(
            limits
                .open(ip("203.0.113.1"), now + Duration::from_secs(10))
                .is_ok(),
            "one more every 10 s"
        );
    }

    #[test]
    fn local_addresses_are_this_network() {
        for local in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.9",
            "192.168.1.5",
            "169.254.3.4",
            "100.64.0.1",
            "100.127.255.254",
            "::1",
            "fd12::1",
            "fe80::1",
            "::ffff:192.168.1.5",
        ] {
            assert!(is_local(ip(local)), "{local}");
        }
        for public in [
            "203.0.113.1",
            "8.8.8.8",
            "100.128.0.1",
            "172.32.0.1",
            "2001:db8::1",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_local(ip(public)), "{public}");
        }
    }

    #[test]
    fn loopback_is_exempt_and_ipv6_counts_by_56() {
        let limits = AddressLimits::new(1, 1);
        let now = Instant::now();
        let _slots: Vec<_> = (0..5)
            .map(|_| limits.open(ip("127.0.0.1"), now).unwrap())
            .collect();
        let _v6 = limits.open(ip("2001:db8:1:2::1"), now).unwrap();
        assert!(limits.open(ip("2001:db8:1:2::ffff"), now).is_err());
        assert!(
            limits.open(ip("2001:db8:1:3::1"), now).is_err(),
            "another /64 of the same /56"
        );
        assert!(limits.open(ip("2001:db8:1:ff::1"), now).is_err());
        assert!(limits.open(ip("2001:db8:1:100::1"), now).is_ok());
    }

    #[test]
    fn proxies_and_forwarded_addresses() {
        let private = TrustedProxies::Private;
        assert!(
            may_proxy(ip("127.0.0.1"), private)
                && may_proxy(ip("172.18.0.3"), private)
                && may_proxy(ip("fd00::1"), private)
        );
        assert!(may_proxy(ip("::ffff:192.168.1.5"), private));
        assert!(!may_proxy(ip("203.0.113.1"), private));
        assert!(
            may_proxy(ip("127.0.0.1"), TrustedProxies::Loopback)
                && may_proxy(ip("::1"), TrustedProxies::Loopback)
        );
        assert!(
            !may_proxy(ip("192.168.1.5"), TrustedProxies::Loopback),
            "a LAN peer is not a proxy by default"
        );
        assert!(!may_proxy(ip("127.0.0.1"), TrustedProxies::None));
        let head = "GET / HTTP/1.1\r\nHost: x\r\nX-Forwarded-For: 198.51.100.7, 203.0.113.9\r\nUpgrade: websocket";
        assert_eq!(forwarded_for(head), Some(ip("203.0.113.9")));
        assert_eq!(forwarded_for("GET / HTTP/1.1\r\nHost: x"), None);
    }
}
