# Online co-op: relay and lobby

Online co-op needs three pieces: `last-harvest-relay` pairs players' WebRTC connections, coturn relays traffic for players whose networks block a direct connection, and the lobby protocol (`crates/lobby`) runs between the host and its clients over `crates/net`'s reliable channel. The host runs the game and sends each client snapshots (`crates/game/src/netsync`); clients draw them and send back their input. The public relay is `wss://lh-relay.vps.pocketsquirrel.com` ([Deploy on the shared VPS](#deploy-on-the-shared-vps)).

## Pieces

| Piece | Where | Role |
|---|---|---|
| `last-harvest-net` | `crates/net` | WebRTC peers in one signaling room, each with a reliable and an unreliable channel. Native and browser. |
| `last-harvest-relay` | `crates/relay` | Matchbox signaling on `/signal/<room>`, which also hands each peer TURN credentials, and `/healthz`. Native only. Copied from supermariowar-rust's `smw_relay`, without its lobby. |
| `last-harvest-lobby` | `crates/lobby` | Room codes and the lobby messages, as state machines with no transport. Builds for wasm. |
| `deploy/` | | The relay's Dockerfile, a compose file and Caddy site block for the shared VPS, coturn's config, and `standalone/` for a fresh host. |

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

## Deploy on the shared VPS

The relay runs at `wss://lh-relay.vps.pocketsquirrel.com` on the VPS that already runs Super Mario War's relay, beside it: SMW's Caddy terminates TLS, and SMW's coturn relays traffic for both games. `deploy/docker-compose.yml` runs only the relay, from the image `.github/workflows/relay.yml` publishes (`ghcr.io/kylejameswalker/last-harvest-relay`, `latest` and the commit's short SHA, for linux/amd64 and linux/arm64) on every push to `main` that touches `crates/relay` or `deploy`.

The wildcard DNS record `*.vps.pocketsquirrel.com` already points at the VPS, and the `caddy` docker network already exists. `<smw-deploy>` below is the directory on the VPS holding SMW's compose file and `.env`.

1. Copy `deploy/docker-compose.yml` and `deploy/Caddyfile` to a new directory on the VPS, such as `~/last-harvest-relay`.
2. Give the relay coturn's secret: `grep '^TURN_SECRET=' <smw-deploy>/.env > ~/last-harvest-relay/.env`. The relay issues TURN credentials with it, so they work on SMW's coturn.
3. Start the relay: `docker compose up -d` in `~/last-harvest-relay`. It publishes no port; Caddy reaches it as `last-harvest-relay:8080` on the `caddy` network.
4. Append the site block to SMW's Caddyfile (`cat Caddyfile >> <smw-deploy>/Caddyfile`; appending keeps the file that Caddy's container mounts), then reload Caddy: `docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile` in `<smw-deploy>`. Caddy gets the certificate on the first request.
5. Check the relay:
   ```sh
   curl https://lh-relay.vps.pocketsquirrel.com/healthz   # ok
   curl -si --http1.1 --max-time 3 -H 'Connection: Upgrade' -H 'Upgrade: websocket' -H 'Sec-WebSocket-Version: 13' \
     -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' -H 'Origin: https://www.kylejameswalker.com' \
     https://lh-relay.vps.pocketsquirrel.com/signal/last-harvest-check | head -1   # HTTP/1.1 101 Switching Protocols
   ```
   The same request with `-H 'Origin: https://example.com'` answers `403`.
6. Run the smoke test from a checkout (no private assets needed): `cargo run -p last-harvest-net --example relay_smoke`. It fetches `/ice`, checks that another page origin is refused and that `https://www.kylejameswalker.com` is accepted with TURN credentials, then joins two native peers and trades a packet on each channel, first directly and then through TURN alone. It ends with `all checks passed`; pass another relay as its first argument, such as `ws://127.0.0.1:8080`.

The relay accepts WebSockets from `https://www.kylejameswalker.com`, where the web game is published (GitHub Pages redirects `kylejameswalker.github.io` there), and from native games, which send no `Origin`. It trusts `X-Forwarded-For` from private addresses (`TRUSTED_PROXIES=private`), which on this host means Caddy on the `caddy` network.

### Hardening the shared coturn

The shared coturn's config lives in supermariowar-rust (`deploy/turnserver.conf`, hardened in KyleJamesWalker/supermariowar-rust#306): UDP relay only (`no-tcp-relay`), 24 allocations per user name (`user-quota`), 300 across the server (`total-quota`) and 19.2 MB/s across all sessions (`bps-capacity`), relay ports 49160-49459. Both relays name TURN credentials per client address, so `user-quota` caps one address in each game. Last Harvest works with SMW's coturn before or after that change.

1. Deploy SMW's relay and its `turnserver.conf` from that PR, following its `docs/guides/RELAY.md` (relay first, then clients).
2. Open the wider range: UDP 49260-49459 (with ufw, `sudo ufw allow 49260:49459/udp`).
3. `docker compose restart coturn` in `<smw-deploy>`. Matches relayed through it at that moment lose their connection.
4. Play one SMW online match and run this repo's smoke test.

`deploy/turnserver.conf` here is the standalone setup's copy; on the shared VPS, SMW's file is the one coturn reads.

### Updating and rolling back

`docker compose pull && docker compose up -d` in `~/last-harvest-relay` takes the newest image. To roll back, pin an earlier one: `RELAY_TAG=<short sha>` in `.env`, then `docker compose up -d`. To take the relay down, `docker compose down` and remove its site block from SMW's Caddyfile, then reload Caddy; SMW is unaffected. To undo the coturn hardening, restore SMW's previous `turnserver.conf` and restart coturn.

### A fresh host

`deploy/standalone/` runs the relay with its own Caddy and coturn, for a host with neither. Point a DNS record at the host, `docker network create caddy`, copy `deploy/standalone/` and `deploy/turnserver.conf` keeping their layout, and create `standalone/.env` from `.env.example`: `RELAY_DOMAIN`, `ALLOWED_ORIGINS` (the web game's page origin) and `TURN_SECRET` (`openssl rand -hex 32`; hex only, because the coturn image evaluates its arguments in a shell). Open TCP and UDP 3478 and UDP 49160-49459 for coturn, which uses host networking, and TCP 80 and 443 for Caddy, then `docker compose up -d` in `standalone/`.

coturn relays only to public addresses (`denied-peer-ip` in `turnserver.conf`), so it cannot reach the server's private network.

## Playing online

Title > Online > Host a night opens a room and shows its code; Join with a code takes it on on-screen keys (pad, touch or keyboard). Settings > Online relay, or Online > Relay, sets the relay; empty means `wss://lh-relay.vps.pocketsquirrel.com`. The game fetches `/ice` from it before joining; TURN credentials arrive over the signaling connection. Every build plays online:

| Platform | Notes |
|---|---|
| macOS, Linux, Windows | On Windows, the first room may bring up Windows Defender Firewall's prompt: allow private networks, so peers on the same network connect directly. |
| RG DS, muOS | With the lid shut, the night goes on without this device's input. |
| Android | Pad or touch. In the background the night goes on as with a lid shut until Android freezes the app; after 10 seconds without word, the host unseats it, or a client leaves. |
| Web | `?relay=`, `?host=` and `?join=`. The relay must allow the page's origin (`LAST_HARVEST_RELAY_ALLOWED_ORIGINS`). |

| Relay setting | Connects to |
|---|---|
| empty | `wss://lh-relay.vps.pocketsquirrel.com` |
| `ws://`, `wss://`, `http://` or `https://` URL | that scheme; `http` means `ws` and `https` means `wss` |
| `host:port` with an IP address, `localhost`, a `.local` name or a name with no dot | `ws://`, for a relay on the LAN |
| any other name, such as `relay.example.com` | `wss://` |

A page served over https, such as the published web game, can open only `wss://`: browsers block `ws://` from it as mixed content. There the game refuses a `ws://` relay or a LAN address before connecting, and says a secure relay is needed. `localhost` and `127.0.0.1` still work, as browsers trust them. To play against a LAN relay from a browser, serve the web build over http (`scripts/web-game.sh --serve`) and allow its origin on the relay.

| Setting | Meaning |
|---|---|
| `LAST_HARVEST_RELAY`, `?relay=` on the web | The relay, over the saved one |
| `LAST_HARVEST_HOST=<code>`, `?host=<code>` on the web | Open room `<code>` at launch (empty for a random code) |
| `LAST_HARVEST_JOIN=<code>`, `?join=` on the web | Join room `<code>` at launch |

A browser against a relay on this Mac: run the relay with `LAST_HARVEST_RELAY_ALLOWED_ORIGINS=http://localhost:8095` (the default), `scripts/web-game.sh --serve`, and open `http://localhost:8095/?relay=127.0.0.1:8080&host=HJKMN`; the start overlay's click joins or hosts.

The join check compares `game_data.pak` fingerprints. A release's web, desktop, handheld and Android builds load the same shared pack and agree. A dev build with the `embed` feature (`cargo run -p last-harvest`) prints the same as `target/game_data.pak` of its commit, so it plays the web build of that commit.

A browser runs the game only while its tab shows: a hidden tab draws no frames, so a web host stalls the night for every client, and a hidden web client stops sending input and turns after 10 seconds. Keep the game's tab in front.

Two windows on one Mac: run the relay, then `LAST_HARVEST_RELAY=127.0.0.1:8080 LAST_HARVEST_HOST=HJKMN target/release/last-harvest` and the same with `LAST_HARVEST_JOIN=HJKMN`. With `LAST_HARVEST_DEBUG`, give each its own `LAST_HARVEST_PIPE` and `LAST_HARVEST_SAVE`.

A client whose player turns leads the dead from its own device, as in split-screen co-op: its pad steers the leader, and it picks the next leader's rise on its own map. A client that drops turns where it stands and frees its seat for a drop-in; it never leads. The host closing the room, or the host's end screen, ends the night for every client.

## Not done yet

- Broken props and fences, the end screen's damage table and damage numbers are the host's alone; a client doesn't see them.
- New farm from the pause menu or the end screen is not offered online.
