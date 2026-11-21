//! The wire form: `LH`, the protocol version, a kind byte, then the fields. Integers are little-endian; strings and
//! lists have a u16 length. A refusal starts with its text in every version, so any client can show it.

use crate::{Hello, Identity, Member, ModPack, PeerKey, Player, Refusal, Seat, Start, PROTOCOL};

const MAGIC: [u8; 2] = *b"LH";
/// The longest string or list a message may carry.
const MAX_LEN: usize = 4096;

const HELLO: u8 = 1;
const READY: u8 = 2;
const CARDS: u8 = 3;
const LEAVE: u8 = 4;
const WELCOME: u8 = 20;
const ROSTER: u8 = 21;
const START: u8 = 22;
const JOINED: u8 = 23;
const LEFT: u8 = 24;
const CLOSED: u8 = 25;
/// Never renumbered: every version reads a refusal.
const REFUSE: u8 = 255;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// Client to host, first.
    Hello(Hello),
    Ready(bool),
    /// New player cards, one per local player.
    Cards(Vec<String>),
    Leave,
    /// Host to client: seated, with these seats; `running` when a run is under way and a `Start` follows.
    Welcome { room: String, seats: Vec<u8>, running: bool },
    Refuse(Refusal),
    Roster(Vec<Member>),
    Start(Start),
    /// A player joined the run in progress.
    Joined(Member),
    /// A device left the run, and its seats are empty.
    Left { peer: PeerKey, seats: Vec<u8> },
    /// The host closed the room.
    Closed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not a lobby message: other traffic on the channel.
    NotLobby,
    /// A lobby message from another protocol version.
    Protocol(u8),
    Kind(u8),
    Truncated,
    TooLong,
    Text,
    Trailing,
}

/// Whether `bytes` is a lobby message of any version.
pub fn is_lobby_message(bytes: &[u8]) -> bool {
    bytes.starts_with(&MAGIC) && bytes.len() >= 4
}

pub fn encode(message: &Message) -> Vec<u8> {
    let mut w = Writer(Vec::with_capacity(64));
    w.0.extend_from_slice(&MAGIC);
    w.u8(PROTOCOL);
    match message {
        Message::Hello(hello) => {
            w.u8(HELLO);
            w.str(&hello.identity.game_version);
            w.u64(hello.identity.pak);
            w.list(&hello.identity.mods, |w, m| {
                w.str(&m.name);
                w.u64(m.fingerprint);
            });
            w.list(&hello.cards, |w, c| w.str(c));
        }
        Message::Ready(ready) => {
            w.u8(READY);
            w.u8(*ready as u8);
        }
        Message::Cards(cards) => {
            w.u8(CARDS);
            w.list(cards, |w, c| w.str(c));
        }
        Message::Leave => w.u8(LEAVE),
        Message::Welcome { room, seats, running } => {
            w.u8(WELCOME);
            w.str(room);
            w.list(seats, |w, s| w.u8(*s));
            w.u8(*running as u8);
        }
        Message::Refuse(refusal) => {
            w.u8(REFUSE);
            w.str(&refusal.to_string());
            refusal_fields(&mut w, refusal);
        }
        Message::Roster(members) => {
            w.u8(ROSTER);
            w.list(members, member);
        }
        Message::Start(start) => {
            w.u8(START);
            w.u64(start.seed);
            w.str(&start.map);
            w.u8(start.difficulty);
            w.list(&start.players, |w, p| {
                w.u8(p.seat);
                w.u128(p.peer);
                w.str(&p.card);
            });
            w.list(&start.unlocked, |w, u| w.str(u));
            w.list(&start.options, |w, (k, v)| {
                w.str(k);
                w.str(v);
            });
        }
        Message::Joined(m) => {
            w.u8(JOINED);
            member(&mut w, m);
        }
        Message::Left { peer, seats } => {
            w.u8(LEFT);
            w.u128(*peer);
            w.list(seats, |w, s| w.u8(*s));
        }
        Message::Closed => w.u8(CLOSED),
    }
    w.0
}

fn member(w: &mut Writer, m: &Member) {
    w.u128(m.peer);
    w.list(&m.seats, |w, s| {
        w.u8(s.seat);
        w.str(&s.card);
    });
    w.u8(m.ready as u8 | (m.host as u8) << 1);
}

fn refusal_fields(w: &mut Writer, refusal: &Refusal) {
    match refusal {
        Refusal::Protocol { host, yours } => {
            w.u8(1);
            w.u8(*host);
            w.u8(*yours);
        }
        Refusal::Version { host, yours } => {
            w.u8(2);
            w.str(host);
            w.str(yours);
        }
        Refusal::Pak => w.u8(3),
        Refusal::Mods { missing, extra, different } => {
            w.u8(4);
            for names in [missing, extra, different] {
                w.list(names, |w, n| w.str(n));
            }
        }
        Refusal::Full { free, asked } => {
            w.u8(5);
            w.u8(*free);
            w.u8(*asked);
        }
        Refusal::Running => w.u8(6),
        Refusal::BadHello(why) => {
            w.u8(7);
            w.str(why);
        }
        Refusal::Other(_) => w.u8(0),
    }
}

pub fn decode(bytes: &[u8]) -> Result<Message, DecodeError> {
    if !is_lobby_message(bytes) {
        return Err(DecodeError::NotLobby);
    }
    let (version, kind) = (bytes[2], bytes[3]);
    let mut r = Reader(&bytes[4..]);
    if kind == REFUSE {
        let text = r.str()?;
        if version != PROTOCOL {
            return Ok(Message::Refuse(Refusal::Other(text)));
        }
        let refusal = read_refusal(&mut r, text)?;
        r.end()?;
        return Ok(Message::Refuse(refusal));
    }
    if version != PROTOCOL {
        return Err(DecodeError::Protocol(version));
    }
    let message = match kind {
        HELLO => {
            let game_version = r.str()?;
            let pak = r.u64()?;
            let mods = r.list(|r| Ok(ModPack { name: r.str()?, fingerprint: r.u64()? }))?;
            let cards = r.list(Reader::str)?;
            Message::Hello(Hello { identity: Identity { game_version, pak, mods }, cards })
        }
        READY => Message::Ready(r.u8()? != 0),
        CARDS => Message::Cards(r.list(Reader::str)?),
        LEAVE => Message::Leave,
        WELCOME => Message::Welcome { room: r.str()?, seats: r.list(Reader::u8)?, running: r.u8()? != 0 },
        ROSTER => Message::Roster(r.list(read_member)?),
        START => Message::Start(Start {
            seed: r.u64()?,
            map: r.str()?,
            difficulty: r.u8()?,
            players: r.list(|r| Ok(Player { seat: r.u8()?, peer: r.u128()?, card: r.str()? }))?,
            unlocked: r.list(Reader::str)?,
            options: r.list(|r| Ok((r.str()?, r.str()?)))?,
        }),
        JOINED => Message::Joined(read_member(&mut r)?),
        LEFT => Message::Left { peer: r.u128()?, seats: r.list(Reader::u8)? },
        CLOSED => Message::Closed,
        other => return Err(DecodeError::Kind(other)),
    };
    r.end()?;
    Ok(message)
}

fn read_member(r: &mut Reader) -> Result<Member, DecodeError> {
    let peer = r.u128()?;
    let seats = r.list(|r| Ok(Seat { seat: r.u8()?, card: r.str()? }))?;
    let flags = r.u8()?;
    Ok(Member { peer, seats, ready: flags & 1 != 0, host: flags & 2 != 0 })
}

fn read_refusal(r: &mut Reader, text: String) -> Result<Refusal, DecodeError> {
    Ok(match r.u8()? {
        1 => Refusal::Protocol { host: r.u8()?, yours: r.u8()? },
        2 => Refusal::Version { host: r.str()?, yours: r.str()? },
        3 => Refusal::Pak,
        4 => Refusal::Mods { missing: r.list(Reader::str)?, extra: r.list(Reader::str)?, different: r.list(Reader::str)? },
        5 => Refusal::Full { free: r.u8()?, asked: r.u8()? },
        6 => Refusal::Running,
        7 => Refusal::BadHello(r.str()?),
        _ => Refusal::Other(text),
    })
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u128(&mut self, v: u128) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        let mut end = s.len().min(MAX_LEN);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.u16(end as u16);
        self.0.extend_from_slice(&s.as_bytes()[..end]);
    }
    fn list<T>(&mut self, items: &[T], mut each: impl FnMut(&mut Writer, &T)) {
        let items = &items[..items.len().min(MAX_LEN)];
        self.u16(items.len() as u16);
        for item in items {
            each(self, item);
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], DecodeError> {
        if self.0.len() < n {
            return Err(DecodeError::Truncated);
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn u128(&mut self) -> Result<u128, DecodeError> {
        Ok(u128::from_le_bytes(self.take(16)?.try_into().unwrap()))
    }
    fn len(&mut self) -> Result<usize, DecodeError> {
        let n = self.u16()? as usize;
        if n > MAX_LEN {
            return Err(DecodeError::TooLong);
        }
        Ok(n)
    }
    fn str(&mut self) -> Result<String, DecodeError> {
        let n = self.len()?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| DecodeError::Text)
    }
    fn list<T>(&mut self, mut each: impl FnMut(&mut Self) -> Result<T, DecodeError>) -> Result<Vec<T>, DecodeError> {
        let n = self.len()?;
        (0..n).map(|_| each(self)).collect()
    }
    fn end(&self) -> Result<(), DecodeError> {
        if self.0.is_empty() { Ok(()) } else { Err(DecodeError::Trailing) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn hello() -> Hello {
        Hello {
            identity: Identity {
                game_version: "v0.2.0".into(),
                pak: 0xfeed,
                mods: vec![ModPack { name: "night-owls".into(), fingerprint: 7 }],
            },
            cards: vec!["farmer".into(), "witch".into()],
        }
    }

    fn every_message() -> Vec<Message> {
        let member = Member { peer: u128::MAX - 3, seats: vec![Seat { seat: 2, card: "witch".into() }], ready: true, host: false };
        vec![
            Message::Hello(hello()),
            Message::Ready(true),
            Message::Ready(false),
            Message::Cards(vec!["farmer".into()]),
            Message::Leave,
            Message::Welcome { room: "AC3JK".into(), seats: vec![1, 2], running: true },
            Message::Roster(vec![member.clone(), Member { host: true, ..member.clone() }]),
            Message::Start(Start {
                seed: 0x1234_5678_9abc_def0,
                map: "farm".into(),
                difficulty: 2,
                players: vec![Player { seat: 0, peer: 1, card: "farmer".into() }, Player { seat: 1, peer: 2, card: "ñandú".into() }],
                unlocked: vec!["scythe".into(), "crows".into()],
                options: vec![("night".into(), "3".into())],
            }),
            Message::Joined(member),
            Message::Left { peer: 99, seats: vec![3] },
            Message::Closed,
            Message::Refuse(Refusal::Protocol { host: 1, yours: 2 }),
            Message::Refuse(Refusal::Version { host: "v1".into(), yours: "v2".into() }),
            Message::Refuse(Refusal::Pak),
            Message::Refuse(Refusal::Mods { missing: vec!["a".into()], extra: vec![], different: vec!["b".into(), "c".into()] }),
            Message::Refuse(Refusal::Full { free: 1, asked: 2 }),
            Message::Refuse(Refusal::Running),
            Message::Refuse(Refusal::BadHello("no cards".into())),
        ]
    }

    #[test]
    fn every_message_reads_back() {
        for message in every_message() {
            let bytes = encode(&message);
            assert!(is_lobby_message(&bytes));
            assert_eq!(bytes[2], PROTOCOL, "{message:?}");
            assert_eq!(decode(&bytes), Ok(message));
        }
    }

    #[test]
    fn a_cut_message_is_truncated_never_a_panic() {
        for message in every_message() {
            let bytes = encode(&message);
            for cut in 4..bytes.len() {
                assert!(decode(&bytes[..cut]).is_err(), "{message:?} cut at {cut}");
            }
        }
        let mut extra = encode(&Message::Leave);
        extra.push(0);
        assert_eq!(decode(&extra), Err(DecodeError::Trailing));
    }

    #[test]
    fn other_traffic_and_other_versions_are_told_apart() {
        assert_eq!(decode(b"snapshot"), Err(DecodeError::NotLobby));
        assert_eq!(decode(b"LH"), Err(DecodeError::NotLobby));
        let mut future = encode(&Message::Hello(hello()));
        future[2] = PROTOCOL + 1;
        assert_eq!(decode(&future), Err(DecodeError::Protocol(PROTOCOL + 1)));
        let mut unknown = encode(&Message::Leave);
        unknown[3] = 99;
        assert_eq!(decode(&unknown), Err(DecodeError::Kind(99)));
    }

    #[test]
    fn a_refusal_from_another_version_reads_as_its_text() {
        let mut bytes = encode(&Message::Refuse(Refusal::Pak));
        bytes[2] = PROTOCOL + 1;
        bytes.extend_from_slice(b"fields this version has never seen");
        assert_eq!(decode(&bytes), Ok(Message::Refuse(Refusal::Other(Refusal::Pak.to_string()))));
    }

    #[test]
    fn long_strings_are_cut_on_a_char_boundary() {
        let long = "é".repeat(MAX_LEN);
        let Ok(Message::Cards(cards)) = decode(&encode(&Message::Cards(vec![long]))) else { panic!() };
        assert!(cards[0].len() <= MAX_LEN && cards[0].chars().all(|c| c == 'é'));
    }
}
