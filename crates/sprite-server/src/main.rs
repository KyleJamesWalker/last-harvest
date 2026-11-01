//! Serves the web build of the sprite editor to devices on the local network, and does for it what a browser
//! can't: reads and saves the sprites folder, and reads its git history.

use std::io::Read;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};

use sprite_editor::history::valid_id;
use sprite_editor::sprite::{self, Sprite, unbundle};
use sprite_editor::store::{FsStore, Store};
use tiny_http::{Header, Method, Response, Server};

const PORT: u16 = 8080;
const MAX_BODY: u64 = 16 << 20;

/// A reply: status, content type and body.
type Reply = (u16, &'static str, Vec<u8>);

fn text(status: u16, body: impl Into<String>) -> Reply {
    (status, "text/plain; charset=utf-8", body.into().into_bytes())
}

/// A file a sprite's folder may hold.
fn sprite_file(file: &str) -> bool {
    let frame = file.strip_suffix(".png").is_some_and(|n| !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()));
    frame || matches!(file, "south.png" | "east.png" | "footprint.png" | "roof.png" | "north.png" | "west.png" | "top.png" | "stand.txt" | "enemy.txt" | "clear.txt" | "paints.txt")
}

fn bytes(found: Option<Vec<u8>>) -> Reply {
    found.map_or_else(|| text(404, "not found"), |b| (200, "application/octet-stream", b))
}

/// Answers one request: the API under `/api/`, else a file of the web build in `web`.
fn handle(store: &FsStore, web: &Path, method: &Method, url: &str, body: &[u8]) -> Reply {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let named = |n: &str| sprite::valid_name(n);
    match (method, parts.as_slice()) {
        (Method::Get, ["api", "names"]) => text(200, store.names().iter().map(|(n, saved)| format!("{n}\t{}\n", *saved as u8)).collect::<String>()),
        (Method::Get, ["api", "file", name, file]) if named(name) && sprite_file(file) => bytes(store.read(name, file)),
        (Method::Get, ["api", "stand", name]) if named(name) => store.stand(name).map_or_else(|| text(404, "no stand"), |s| text(200, s)),
        (Method::Put, ["api", "stand", name]) if named(name) => {
            // Written back from its parsed form, so nothing else reaches the catalog.
            let how = std::str::from_utf8(body).ok().map(str::trim).and_then(|h| if h == "carve" || last_harvest_game::turned_depth(h).is_some() { Some(h.to_string()) } else { sprite::parse_stand(h).map(sprite::stand_text) });
            match how {
                Some(how) => store.save_stand(name, &how).map_or_else(|e| text(500, e), |()| text(200, "saved")),
                None => text(400, "a stand is prop N, slab N, figure, ground, carve or turned"),
            }
        }
        (Method::Put, ["api", "sprite", name]) if named(name) => {
            let Some(files) = unbundle(body).filter(|f| f.iter().all(|(n, _)| sprite_file(n) && n != "stand.txt")) else { return text(400, "bad bundle") };
            match Sprite::from_files(name, &files) {
                Some(s) if s.frames.len() == files.iter().filter(|(n, _)| n.as_bytes()[0].is_ascii_digit()).count() => store.save(&s).map_or_else(|e| text(500, e), |()| text(200, "saved")),
                _ => text(400, "frames must run 0.png, 1.png and on, and decode"),
            }
        }
        (Method::Get, ["api", "history", name]) if named(name) => match store.history(name) {
            Some((commits, changed)) => text(200, std::iter::once((changed as u8).to_string()).chain(commits.iter().map(|c| c.line())).collect::<Vec<_>>().join("\n")),
            None => text(404, "not a git repository"),
        },
        (Method::Get, ["api", "version", id, name, file]) if valid_id(id) && named(name) && sprite_file(file) => bytes(store.version(id, name, file)),
        (Method::Get, ["api", "table", file]) if last_harvest_game::TABLES.contains(file) => store.read_table(file).map_or_else(|| text(404, "no such table"), |t| text(200, t)),
        (Method::Put, ["api", "table", file]) if last_harvest_game::TABLES.contains(file) => match std::str::from_utf8(body) {
            // Checked as the game reads it, so a save never breaks the build.
            Ok(t) => store.save_table(file, t).map_or_else(|e| text(400, e), |()| text(200, "saved")),
            Err(_) => text(400, "not text"),
        },
        // lospec.com sends no CORS header, so the page asks this server to fetch a palette for it.
        (Method::Get, ["api", "lospec", name]) => store.lospec(name).map_or_else(|e| text(502, e), |t| text(200, t)),
        (Method::Get, ["api", "maps"]) => text(200, store.map_ids().iter().map(|id| format!("{id}\n")).collect::<String>()),
        (Method::Get, ["api", "map", id]) if last_harvest_game::valid_map_id(id) => store.read_map(id).map_or_else(|| text(404, "no such map"), |t| text(200, t)),
        (Method::Put, ["api", "map", id]) if last_harvest_game::valid_map_id(id) => match std::str::from_utf8(body).map_err(|_| "not text".to_string()).and_then(last_harvest_game::MapSource::parse) {
            // Written back from its parsed form, so only map syntax reaches the folder.
            Ok(src) => store.save_map(id, &src.to_text()).map_or_else(|e| text(500, e), |()| text(200, "saved")),
            Err(e) => text(400, e),
        },
        (_, ["api", ..]) => text(404, "no such call"),
        (Method::Get, [file]) => {
            let file = if file.is_empty() { "index.html" } else { file };
            let kind = match file.rsplit('.').next() {
                Some("html") => "text/html; charset=utf-8",
                Some("js") => "text/javascript",
                Some("wasm") => "application/wasm",
                Some("css") => "text/css",
                Some("png") => "image/png",
                _ => return text(404, "not found"),
            };
            match std::fs::read(web.join(file)) {
                Ok(b) if !file.starts_with('.') => (200, kind, b),
                _ => text(404, format!("{file} not found: build the web editor with scripts/web-editor.sh")),
            }
        }
        _ => text(404, "not found"),
    }
}

/// Loopback and private addresses: the tileset's license keeps the web build, which embeds it, off the internet.
fn local(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(a) => a.is_loopback() || a.is_private() || a.is_link_local(),
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(v4) => local(IpAddr::V4(v4)),
            None => a.is_loopback() || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80,
        },
    }
}

/// This machine's address on the local network: where it would route to the outside from, with nothing sent.
fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    Some(s.local_addr().ok()?.ip())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port = std::env::var("SPRITE_SERVER_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(PORT);
    let web = args.first().map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/web-editor"));
    let store = FsStore::default();
    let server = Server::http(SocketAddr::from(([0, 0, 0, 0], port))).unwrap_or_else(|e| panic!("port {port}: {e}"));
    eprintln!("sprites: {}\nweb build: {}", store.dir.display(), web.display());
    if !web.join("index.html").exists() {
        eprintln!("no web build there yet: run scripts/web-editor.sh");
    }
    match lan_ip() {
        Some(ip) if local(ip) => eprintln!("open http://{ip}:{port}/ on the iPad, or http://localhost:{port}/ here"),
        _ => eprintln!("open http://localhost:{port}/; no local network address found"),
    }
    for mut req in server.incoming_requests() {
        if !req.remote_addr().is_some_and(|a| local(a.ip())) {
            let _ = req.respond(Response::from_string("local network only").with_status_code(403));
            continue;
        }
        let mut body = Vec::new();
        if req.as_reader().take(MAX_BODY).read_to_end(&mut body).is_err() {
            continue;
        }
        let (method, url) = (req.method().clone(), req.url().to_string());
        let (status, kind, out) = handle(&store, &web, &method, &url, &body);
        if method != Method::Get || status >= 400 {
            eprintln!("{method} {url} -> {status}");
        }
        let response = Response::from_data(out)
            .with_status_code(status)
            .with_header(Header::from_bytes("Content-Type", kind).unwrap())
            .with_header(Header::from_bytes("Cache-Control", "no-store").unwrap());
        let _ = req.respond(response);
    }
}

#[cfg(test)]
mod tests {
    use sprite_editor::sprite::{Frame, bundle};

    use super::*;

    fn store(tag: &str) -> FsStore {
        let root = std::env::temp_dir().join(format!("lh-server-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("art.txt"), "BARREL  cell 14 7\n").unwrap();
        FsStore { dir: root.join("sprites"), catalog: root.join("art.txt"), cards: root.join("enemies") }
    }

    #[test]
    fn saves_and_reads_back_a_sprite_the_way_the_editor_does() {
        let s = store("save");
        let web = Path::new("/nonexistent");
        let mut sprite = Sprite { name: "BARREL".into(), frames: vec![Frame::blank(4, 3); 3], views: None, footprint: None, roof: None, north: None, west: None, top: Some(Frame::blank(4, 1)), card: None, clear_in_3d: Vec::new(), paint: Default::default() };
        sprite.frames[2].set(1, 1, [9, 8, 7, 255]);
        assert_eq!(handle(&s, web, &Method::Put, "/api/sprite/BARREL", &bundle(&sprite.files())).0, 200);
        sprite.frames.truncate(2);
        assert_eq!(handle(&s, web, &Method::Put, "/api/sprite/BARREL", &bundle(&sprite.files())).0, 200);
        assert!(s.dir.join("BARREL/1.png").exists() && !s.dir.join("BARREL/2.png").exists(), "a shorter save drops leftovers");
        let (status, _, png) = handle(&s, web, &Method::Get, "/api/file/BARREL/1.png", &[]);
        assert_eq!((status, png), (200, std::fs::read(s.dir.join("BARREL/1.png")).unwrap()));
        assert_eq!(handle(&s, web, &Method::Get, "/api/file/BARREL/top.png", &[]).0, 200, "the top strip comes back too");
        assert_eq!(handle(&s, web, &Method::Put, "/api/stand/BARREL", b"slab 7").0, 200);
        assert_eq!(std::fs::read_to_string(&s.catalog).unwrap(), "BARREL  cell 14 7  @slab 7\n");
        assert_eq!(handle(&s, web, &Method::Get, "/api/stand/BARREL", &[]).2, b"slab 7");
        assert!(String::from_utf8(handle(&s, web, &Method::Get, "/api/names", &[]).2).unwrap().contains("BARREL\t1\n"));
        std::fs::remove_dir_all(s.dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn saves_and_reads_back_a_card_part_the_way_the_web_editor_does() {
        let s = store("card");
        let web = Path::new("/nonexistent");
        let mut part = Sprite { name: "CARD_GHOUL_HURT".into(), frames: vec![Frame::blank(16, 16); 3], views: None, footprint: None, roof: None, north: None, west: None, top: None, card: Some("[ghoul]\nhp 40\n".into()), clear_in_3d: Vec::new(), paint: Default::default() };
        part.frames[2].set(7, 7, [1, 2, 3, 255]);
        assert_eq!(handle(&s, web, &Method::Put, "/api/sprite/CARD_GHOUL_HURT", &bundle(&part.files())).0, 200);
        assert!(s.cards.join("ghoul.enemy.png").exists());
        assert_eq!(handle(&s, web, &Method::Get, "/api/file/CARD_GHOUL_SIDE/enemy.txt", &[]).2, b"[ghoul]\nhp 40\n");
        let back = Sprite::load_from("CARD_GHOUL_HURT", |file| Some(handle(&s, web, &Method::Get, &format!("/api/file/CARD_GHOUL_HURT/{file}"), &[])).filter(|r| r.0 == 200).map(|r| r.2)).unwrap();
        assert_eq!((back.frames.len(), back.frames[2].get(7, 7)), (3, Some([1, 2, 3, 255])));
        let names = String::from_utf8(handle(&s, web, &Method::Get, "/api/names", &[]).2).unwrap();
        assert!(["FRONT", "SIDE", "BACK", "FLASH", "HURT"].iter().all(|p| names.contains(&format!("CARD_GHOUL_{p}\t1\n"))));
        let before = std::fs::read(s.cards.join("ghoul.enemy.png")).unwrap();
        part.frames.pop();
        assert_eq!(handle(&s, web, &Method::Put, "/api/sprite/CARD_GHOUL_HURT", &bundle(&part.files())).0, 500, "a part keeps its frames");
        assert_eq!(std::fs::read(s.cards.join("ghoul.enemy.png")).unwrap(), before, "and a refused save leaves the card as it was");
        std::fs::remove_dir_all(s.dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_sprite_save_refuses_pictures_that_dont_decode() {
        let s = store("undecodable");
        let web = Path::new("/nonexistent");
        let good = sprite::encode_png(&Frame::blank(4, 3));
        for files in [vec![("0.png", b"junk".to_vec())], vec![("0.png", good.clone()), ("1.png", b"junk".to_vec())], vec![("0.png", good.clone()), ("south.png", b"junk".to_vec())], vec![("0.png", good.clone()), ("2.png", good.clone())]] {
            let files: Vec<(String, Vec<u8>)> = files.into_iter().map(|(n, b)| (n.to_string(), b)).collect();
            assert_eq!(handle(&s, web, &Method::Put, "/api/sprite/BARREL", &bundle(&files)).0, 400, "{:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>());
        }
        assert!(!s.dir.exists(), "nothing is written");
        std::fs::remove_dir_all(s.dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn refuses_what_could_reach_outside_a_sprite_folder() {
        let s = store("refuse");
        let web = Path::new("/nonexistent");
        for url in ["/api/file/../art.txt", "/api/file/BARREL/..", "/api/file/barrel/0.png", "/api/version/HEAD/BARREL/0.png", "/api/file/BARREL/x.txt"] {
            assert_eq!(handle(&s, web, &Method::Get, url, &[]).0, 404, "{url}");
        }
        let bad = bundle(&[("../0.png".into(), vec![])]);
        assert_eq!(handle(&s, web, &Method::Put, "/api/sprite/BARREL", &bad).0, 400);
        assert_eq!(handle(&s, web, &Method::Put, "/api/stand/BARREL", b"prop\n@slab 3").0, 400);
        assert_eq!(handle(&s, web, &Method::Put, "/api/stand/BARREL", b"prop 3 4\nSNEAKY").0, 200);
        assert_eq!(std::fs::read_to_string(&s.catalog).unwrap(), "BARREL  cell 14 7  @prop 3\n");
        assert!(!s.dir.exists());
        assert!(!local("8.8.8.8".parse().unwrap()) && local("192.168.1.5".parse().unwrap()) && local("::1".parse().unwrap()));
        std::fs::remove_dir_all(s.dir.parent().unwrap()).unwrap();
    }
}
