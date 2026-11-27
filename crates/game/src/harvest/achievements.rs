//! The achievements `achievements.txt` lists, and which of them a player's records have earned.

use std::sync::OnceLock;

use super::powerups::{Progress, Unlock};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Achievement {
    pub id: String,
    pub name: String,
    pub when: Unlock,
}

impl Achievement {
    /// What it takes, in words.
    pub fn about(&self) -> String {
        self.when.text()
    }
}

pub const TEXT: &str = include_str!("../../achievements.txt");

/// Reads `achievements.txt`; errors name the line.
pub fn parse(text: &str) -> Result<Vec<Achievement>, String> {
    let mut out: Vec<Achievement> = Vec::new();
    let mut whens: Vec<Option<Unlock>> = Vec::new();
    let finish = |out: &[Achievement], whens: &[Option<Unlock>]| match (out.last(), whens.last()) {
        (Some(a), Some(None)) => Err(format!("[{}] has no when line", a.id)),
        (Some(a), _) if a.name.is_empty() => Err(format!("[{}] has no name", a.id)),
        _ => Ok(()),
    };
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            finish(&out, &whens).map_err(|e| format!("{n}: {e}"))?;
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(format!(
                    "{n}: [{id}]: an id is lowercase letters, digits and _"
                ));
            }
            if out.iter().any(|a| a.id == id) {
                return Err(format!("{n}: [{id}] appears twice"));
            }
            out.push(Achievement {
                id: id.into(),
                name: String::new(),
                when: Unlock::Start,
            });
            whens.push(None);
            continue;
        }
        let (Some(a), Some(when)) = (out.last_mut(), whens.last_mut()) else {
            return Err(format!("{n}: a line before the first [id]"));
        };
        let (key, rest) = line
            .split_once(char::is_whitespace)
            .map_or((line, ""), |(k, r)| (k, r.trim()));
        match key {
            "name" if !rest.is_empty() => a.name = rest.into(),
            "when" => {
                let w = Unlock::parse(key, rest).map_err(|e| format!("{n}: {e}"))?;
                if w == Unlock::Start {
                    return Err(format!("{n}: [{}] would unlock from the start", a.id));
                }
                a.when = w;
                *when = Some(w);
            }
            _ => return Err(format!("{n}: {key:?} isn't name or when, with a value")),
        }
    }
    finish(&out, &whens).map_err(|e| format!("end: {e}"))?;
    Ok(out)
}

/// The game's own achievements.
pub fn all() -> &'static [Achievement] {
    static ALL: OnceLock<Vec<Achievement>> = OnceLock::new();
    ALL.get_or_init(|| parse(TEXT).unwrap_or_default())
}

/// The ids of every achievement `p` has earned. Unlocking everything for a try-out earns none.
pub fn reached(p: &Progress) -> impl Iterator<Item = &'static str> {
    let p = Progress {
        everything: false,
        ..*p
    };
    all()
        .iter()
        .filter(move |a| a.when.met(&p))
        .map(|a| a.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_achievement_names_its_line() {
        let fails = |text: &str| parse(text).unwrap_err();
        assert_eq!(fails("name X"), "1: a line before the first [id]");
        assert_eq!(fails("[a]\nname A"), "end: [a] has no when line");
        assert_eq!(fails("[a]\nwhen nights 1\n[b]"), "3: [a] has no name");
        assert_eq!(
            fails("[A]"),
            "1: [A]: an id is lowercase letters, digits and _"
        );
        assert_eq!(
            fails("[a]\nname A\nwhen nights 1\n[a]"),
            "4: [a] appears twice"
        );
        assert_eq!(
            fails("[a]\nname A\nwhen start"),
            "3: [a] would unlock from the start"
        );
        assert!(fails("[a]\nname A\nwhen kills lots").starts_with("3: \"lots\""));
        assert!(fails("[a]\nname A\nicon X").starts_with("3: \"icon\""));
    }

    #[test]
    fn only_records_that_meet_a_condition_reach_it() {
        let a = parse("[k]\nname K\nwhen kills 10\n[l]\nname L # level\nwhen level 3").unwrap();
        assert_eq!(
            (a[1].name.as_str(), a[0].about()),
            ("L", Unlock::Kills(10).text())
        );
        assert_eq!(reached(&Progress::default()).count(), 0);
        let all_on = Progress {
            everything: true,
            ..Progress::default()
        };
        assert_eq!(reached(&all_on).count(), 0, "a try-out earns nothing");
        let first = all().first().expect("the game has achievements");
        assert_eq!(first.when, Unlock::Nights(1));
        let one_night = Progress {
            nights: 1,
            ..Progress::default()
        };
        assert!(reached(&one_night).any(|id| id == first.id));
    }
}
