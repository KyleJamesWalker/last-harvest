# Online co-op: relay and lobby

Online co-op needs three pieces: `last-harvest-relay` pairs players' WebRTC connections, coturn relays traffic for players whose networks block a direct connection, and the lobby protocol (`crates/lobby`) runs between the host and its clients over `crates/net`'s reliable channel. The host runs the game and sends each client snapshots (`crates/game/src/netsync`); clients draw them and send back their input. Nothing is deployed yet.

## Pieces

| Piece | Where | Role |
|---|---|---|
| `last-harvest-net` | `crates/net` | WebRTC peers in one signaling room, each with a reliable and an unreliable channel. Native and browser. |
| `last-harvest-relay` | `crates/relay` | Matchbox signaling on `/signal/<room>`, which also hands each peer TURN credentials, and `/healthz`. Native only. Copied from supermariowar-rust's `smw_relay`, without its lobby. |
| `last-harvest-lobby` | `crates/lobby` | Room codes and the lobby messages, as state machines with no transport. Builds for wasm. |
| `deploy/` | | A Dockerfile and a compose file that run the relay behind Caddy, with coturn. |

A room code names a signaling room: `RoomCode::room_url("wss://<relay>")` is `wss://<relay>/signal/last-harvest-<CODE>`. The host and every client connect a `PeerSocket` to that URL, and the relay pairs them into a full mesh. Game traffic never passes through the relay; it goes peer to peer, or through coturn.

## Running the relay locally

```sh
cargo run -p last-harvest-relay -- --port 8080
curl http://localhost:8080/healthz   # ok
curl http://localhost:8080/ice       # {"iceServers":[]}: TURN credentials go only to signaling peers
```

Peers on one network need no TURN server: connect each `PeerSocket` to `ws://<host>:8080/signal/last-harvest-<CODE>` with `IceConfig::default()`. `cargo test -p last-harvest-lobby --test relay` runs the relay and three peers in one process through create, join, ready, start and close.

To try the container instead (the build context is the repository root):

```sh
docker build -f deploy/relay.Dockerfile -t last-harvest-relay:local .
docker run --rm -p 127.0.0.1:8080:8080 last-harvest-relay:local
```

## Relay settings

| Setting | Default |
|---|---|
| `--port`, `LAST_HARVEST_RELAY_PORT` | `8080` |
| `LAST_HARVEST_RELAY_ALLOWED_ORIGINS` | `http://localhost:8095` (`scripts/web-game.sh --serve`), or `*` for any |
| `LAST_HARVEST_RELAY_MAX_CLIENTS` | `128` signaling peers |
| `LAST_HARVEST_RELAY_MAX_CONNECTIONS` | `256` TCP connections of any kind, handshakes included |
| `LAST_HARVEST_RELAY_MAX_PER_ADDRESS` | `16` connections from one client address; `0` for no limit |
| `LAST_HARVEST_RELAY_MAX_PEERS_PER_ADDRESS` | `8` signaling peers from one client address; `0` for no limit |
| `LAST_HARVEST_RELAY_LONELY_SECONDS` | `900`: a signaling peer alone in its room this long is closed |
| `LAST_HARVEST_RELAY_CONNECTS_PER_MINUTE` | `60` new connections per client address per minute; `0` for no limit |
| `LAST_HARVEST_RELAY_TRUSTED_PROXIES` | `loopback`, `private` or `none`: the peers whose `X-Forwarded-For` names the client |
| `LAST_HARVEST_RELAY_TURN_URL` | unset; the TURN server signaling peers get, such as `turn:relay.example.com:3478` |
| `LAST_HARVEST_RELAY_TURN_SECRET` | unset; coturn's `static-auth-secret` |
| `LAST_HARVEST_RELAY_TURN_SECONDS` | `1200`: how long a TURN credential lasts |

The relay refuses a WebSocket or an `/ice` request whose `Origin` is not allowed, and accepts one without `Origin` (a native game). Per-address limits count an IPv6 client by its /56, and loopback clients are exempt. A signaling room holds four peers.

A peer whose connection drops without a close keeps its place for 30 seconds. To take it back, a client connects to `/signal/<room>?resume` and sends `{"Resume": {"id": <id>, "token": <token>}}` first, with the `ResumeToken` the relay sent; the token never appears in a URL, so access logs cannot hold it. `crates/net` does this on its own.

With a TURN URL and secret set, the relay sends each signaling peer `{"IceServers": {"iceServers": [...]}}` after `IdAssigned` and `ResumeToken`, and a fresh one every quarter of `LAST_HARVEST_RELAY_TURN_SECONDS` while the peer stays connected. Credentials use coturn's `use-auth-secret` scheme; the name after the expiry is derived from the client's address, so every peer from one address shares coturn's `user-quota`. `crates/net` adds these servers to the game's own for each new connection, and a browser's live connections use them on their next ICE restart. A native connection keeps the credentials it was made with: an ICE restart more than 20 minutes after it was made gathers no TURN candidate, though an allocation already in use lasts. `/ice` still answers, with no servers, for clients that fetch it before joining.

## Lobby protocol

Every message starts with `LH`, the protocol version (`PROTOCOL`) and a kind byte. A message without `LH` is other traffic, and the lobby ignores it. A refusal starts with its text in every version, so a client on another release can still show why the host turned it away.

| Message | Direction | Content |
|---|---|---|
| `Hello` | client to every peer | game version, `game_data.pak` fingerprint, mod packs by name and fingerprint, one player card per local player (1 or 2) |
| `Welcome` | host | the room code, this device's seats, whether a run is under way |
| `Refuse` | host | a reason: protocol, version, pak, mods (missing, extra, different), full, running, bad hello |
| `Roster` | host | every device, its seats and cards, its ready mark; the host first |
| `Ready`, `Cards` | client | a ready mark; new player cards |
| `Start` | host | seed, map, difficulty, every player by seat, the unlocked powerup pool, other options |
| `Joined` | host | a device that joined the run in progress |
| `Left` | host | a device that left the run, and its emptied seats |
| `Leave` | client | this device leaves |
| `Closed` | host | the host closed the room |

The flow:

1. The host picks a code (`RoomCode::from_random`), connects to its room, and builds a `Host` with its own peer ID.
2. A client parses the typed code (`RoomCode::parse`: any case, spaces and dashes ignored, look-alike characters refused), connects, and sends `Hello` to every peer it connects to. The peer that answers is the host; other clients ignore a `Hello`.
3. The host compares the hello with its own identity and free seats, then sends `Welcome` and the roster, or `Refuse`.
4. Players mark themselves ready. `Host::start` fails until every device is ready, then sends `Start` to every client.
5. A client that connects during a run gets `Welcome` and `Start`, and the other clients get `Joined`: everyone spawns its players. A device that leaves or disconnects empties its seats (`Left`).
6. `Host::close` sends `Closed`, and every client ends. A client whose transport loses the host ends too.

Seats run from 0 to 3 over every device. A client seats one or two players for split-screen in an online game. A host that hears `Welcome` or `Roster` from another peer has picked a code already in use (`HostEvent::CodeTaken`) and should pick another.

## Deploying

Not done yet. These steps describe how the files in `deploy/` would run on a server with Docker.

1. Point a DNS record for the relay's domain at the server.
2. On the server, create the shared network: `docker network create caddy`.
3. Copy the repository (the compose file builds the image from it), then create `deploy/.env` from `deploy/.env.example`: `RELAY_DOMAIN`, `ALLOWED_ORIGINS` (the web game's page origin) and `TURN_SECRET` (`openssl rand -hex 32`; hex only, because the coturn image evaluates its arguments in a shell). Keep `.env` out of git.
4. Open TCP and UDP port 3478 and UDP ports 49160-49459 for coturn, which uses host networking, and TCP 80 and 443 for Caddy.
5. Run `docker compose up -d` in `deploy/`. Caddy gets the certificate and proxies to the relay. If Caddy already runs on the `caddy` network, add the Caddyfile's site block to its config and remove the `caddy` service.
6. Check `curl https://<domain>/healthz` returns `ok`, and that a game's log shows the TURN server once it joins a room.

coturn relays only to public addresses (`denied-peer-ip` in `turnserver.conf`), so it cannot reach the server's private network. It relays UDP only (`no-tcp-relay`). One client address holds at most 24 allocations (`user-quota`), the server 200 (`total-quota`), each at most 64 KB/s each way (`max-bps`, within `bps-capacity`).

## Playing online

Title > Online > Host a night opens a room and shows its code; Join with a code takes it on on-screen keys (pad, touch or keyboard). Settings > Online relay, or Online > Relay, sets the relay as `host:port` (a LAN address works) or a `ws`, `wss`, `http` or `https` URL; empty means `localhost:8080`. The game fetches `/ice` from it before joining; TURN credentials arrive over the signaling connection. Online is on for macOS, Linux (the RG DS and muOS builds included) and the web, and off on Android and Windows.

| Setting | Meaning |
|---|---|
| `LAST_HARVEST_RELAY`, `?relay=` on the web | The relay, over the saved one |
| `LAST_HARVEST_HOST=<code>` | Open room `<code>` at launch (empty for a random code) |
| `LAST_HARVEST_JOIN=<code>`, `?join=` on the web | Join room `<code>` at launch |

Two windows on one Mac: run the relay, then `LAST_HARVEST_RELAY=127.0.0.1:8080 LAST_HARVEST_HOST=HJKMN target/release/last-harvest` and the same with `LAST_HARVEST_JOIN=HJKMN`. With `LAST_HARVEST_DEBUG`, give each its own `LAST_HARVEST_PIPE` and `LAST_HARVEST_SAVE`.

A client whose player turns leads the dead from its own device, as in split-screen co-op: its pad steers the leader, and it picks the next leader's rise on its own map. A client that drops turns where it stands and frees its seat for a drop-in; it never leads. The host closing the room, or the host's end screen, ends the night for every client.

## Not done yet

- A deployed relay.
- Broken props and fences, the end screen's damage table and damage numbers are the host's alone; a client doesn't see them.
- New farm from the pause menu or the end screen is not offered online.
