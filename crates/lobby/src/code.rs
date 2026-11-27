//! Room codes: short, typed with a gamepad or a touch screen, with no two characters that look alike.

/// The characters a code is made of: no B/8, G/6, I/L/1, O/Q/0, S/5, U/V, Z/2.
pub const ALPHABET: &[u8; 21] = b"ACDEFHJKMNPRTWXY34679";
pub const CODE_LEN: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoomCode([u8; CODE_LEN]);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeError {
    Length(usize),
    Character(char),
}

impl std::fmt::Display for CodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeError::Length(n) => write!(f, "A room code has {CODE_LEN} characters, not {n}."),
            CodeError::Character(c) => write!(f, "'{c}' is not in room codes."),
        }
    }
}

impl RoomCode {
    /// A code from random bits, such as the game's `Rng`.
    pub fn from_random(mut bits: u64) -> RoomCode {
        let mut code = [0; CODE_LEN];
        for c in &mut code {
            *c = ALPHABET[(bits % ALPHABET.len() as u64) as usize];
            bits /= ALPHABET.len() as u64;
        }
        RoomCode(code)
    }

    /// A typed code: any case, spaces and dashes ignored.
    pub fn parse(text: &str) -> Result<RoomCode, CodeError> {
        let chars: Vec<char> = text
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if chars.len() != CODE_LEN {
            return Err(CodeError::Length(chars.len()));
        }
        let mut code = [0; CODE_LEN];
        for (slot, c) in code.iter_mut().zip(chars) {
            *slot = u8::try_from(c)
                .ok()
                .filter(|b| ALPHABET.contains(b))
                .ok_or(CodeError::Character(c))?;
        }
        Ok(RoomCode(code))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("codes are ASCII")
    }

    /// The signaling room URL on a relay such as `wss://relay.example.com`.
    pub fn room_url(&self, relay: &str) -> String {
        format!(
            "{}/signal/last-harvest-{}",
            relay.trim_end_matches('/'),
            self.as_str()
        )
    }
}

impl std::fmt::Display for RoomCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_read_back_from_what_a_player_types() {
        for bits in [0, 1, 12345, u64::MAX, 0x9e37_79b9_7f4a_7c15] {
            let code = RoomCode::from_random(bits);
            assert_eq!(RoomCode::parse(code.as_str()), Ok(code));
            assert_eq!(RoomCode::parse(&code.as_str().to_lowercase()), Ok(code));
        }
        assert_eq!(RoomCode::parse(" ac-3jk "), RoomCode::parse("AC3JK"));
        assert_ne!(RoomCode::from_random(1), RoomCode::from_random(2));
    }

    #[test]
    fn look_alike_characters_are_refused() {
        for c in [
            'O', '0', 'I', '1', 'L', 'S', '5', 'Z', '2', 'B', '8', 'G', 'é',
        ] {
            assert_eq!(
                RoomCode::parse(&format!("ACDE{c}")),
                Err(CodeError::Character(c))
            );
        }
        assert_eq!(RoomCode::parse("ACD"), Err(CodeError::Length(3)));
        assert_eq!(
            CodeError::Length(3).to_string(),
            "A room code has 5 characters, not 3."
        );
    }

    #[test]
    fn a_code_names_a_signaling_room() {
        let code = RoomCode::parse("AC3JK").unwrap();
        assert_eq!(
            code.room_url("wss://relay.example.com/"),
            "wss://relay.example.com/signal/last-harvest-AC3JK"
        );
    }
}
