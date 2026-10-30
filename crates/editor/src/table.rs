//! A data file's text, such as `tuning.txt`, as the data page edits it: `[section]` headings over `key value`
//! lines, each line changed in place so its comment and the rest of the file stay as written.

/// One `key value` line of a section, with the comment after it and where it sits in the text.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub comment: String,
    line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub file: String,
    pub text: String,
}

/// Lines before the first heading list under this name.
pub const TOP: &str = "";

fn content(line: &str) -> (&str, &str) {
    match line.split_once('#') {
        Some((c, comment)) => (c.trim(), comment.trim()),
        None => (line.trim(), ""),
    }
}

fn heading(line: &str) -> Option<&str> {
    content(line).0.strip_prefix('[').and_then(|l| l.strip_suffix(']'))
}

impl Table {
    pub fn new(file: &str, text: &str) -> Self {
        Self { file: file.to_string(), text: text.to_string() }
    }

    /// Every section by its heading, with `TOP` first when lines come before any heading.
    pub fn sections(&self) -> Vec<String> {
        let mut out = Vec::new();
        for line in self.text.lines() {
            match heading(line) {
                Some(h) => out.push(h.to_string()),
                None if out.is_empty() && !content(line).0.is_empty() => out.push(TOP.to_string()),
                None => {}
            }
        }
        out
    }

    /// The lines of `section`: from its heading, or the top, to the next heading.
    fn span(&self, section: &str) -> Option<(usize, usize)> {
        let lines: Vec<&str> = self.text.lines().collect();
        let start = if section == TOP { 0 } else { lines.iter().position(|l| heading(l) == Some(section))? + 1 };
        let end = (start..lines.len()).find(|&i| heading(lines[i]).is_some()).unwrap_or(lines.len());
        Some((start, end))
    }

    pub fn entries(&self, section: &str) -> Vec<Entry> {
        let Some((start, end)) = self.span(section) else { return Vec::new() };
        let lines: Vec<&str> = self.text.lines().collect();
        (start..end)
            .filter_map(|i| {
                let (c, comment) = content(lines[i]);
                let (key, value) = c.split_once(char::is_whitespace).map_or((c, ""), |(k, v)| (k, v.trim()));
                (!key.is_empty()).then(|| Entry { key: key.into(), value: value.into(), comment: comment.into(), line: i })
            })
            .collect()
    }

    fn rewrite(&mut self, f: impl FnOnce(&mut Vec<String>)) {
        let ends = self.text.ends_with('\n');
        let mut lines: Vec<String> = self.text.lines().map(str::to_string).collect();
        f(&mut lines);
        self.text = lines.join("\n");
        if ends {
            self.text.push('\n');
        }
    }

    /// Sets the value of the section's `at`th line, keeping its comment where it was.
    pub fn set(&mut self, section: &str, at: usize, value: &str) -> bool {
        let Some(e) = self.entries(section).into_iter().nth(at) else { return false };
        self.rewrite(|lines| {
            let old = &lines[e.line];
            let indent = &old[..old.len() - old.trim_start().len()];
            let key = &e.key;
            let body = if value.is_empty() { key.to_string() } else { format!("{key} {value}") };
            let line = match old.find('#') {
                // The comment keeps its column when the new value leaves room for it.
                Some(at) if body.len() + indent.len() < at => format!("{indent}{body:<w$}{}", &old[at..], w = at - indent.len()),
                Some(at) => format!("{indent}{body} {}", &old[at..]),
                None => format!("{indent}{body}"),
            };
            lines[e.line] = line;
        });
        true
    }

    /// Adds a line at the end of `section`, after its last line.
    pub fn add(&mut self, section: &str, key: &str, value: &str) -> bool {
        let Some((start, _)) = self.span(section) else { return false };
        let at = self.entries(section).last().map_or(start, |e| e.line + 1);
        self.rewrite(|lines| lines.insert(at, if value.is_empty() { key.to_string() } else { format!("{key} {value}") }));
        true
    }

    pub fn remove(&mut self, section: &str, at: usize) -> bool {
        let Some(e) = self.entries(section).into_iter().nth(at) else { return false };
        self.rewrite(|lines| {
            lines.remove(e.line);
        });
        true
    }
}

/// A number one step up or down: by 1 for whole numbers, else by its last decimal place; Shift steps ten times as far.
pub fn step(value: &str, by: i32, shift: bool) -> Option<String> {
    let v: f64 = value.parse().ok()?;
    let places = value.split_once('.').map_or(0, |(_, d)| d.len());
    let unit = 10f64.powi(-(places as i32)) * if shift { 10.0 } else { 1.0 };
    let next = (v + unit * by as f64).max(0.0);
    Some(format!("{next:.places$}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# numbers\nslots 6\n\n[player]\nhp 100          # health\nspeed 48\n\n[easy]\ndamage 0.55\n";

    #[test]
    fn sections_and_lines_read_with_their_comments() {
        let t = Table::new("tuning.txt", TEXT);
        assert_eq!(t.sections(), ["", "player", "easy"]);
        let player = t.entries("player");
        assert_eq!((player[0].key.as_str(), player[0].value.as_str(), player[0].comment.as_str()), ("hp", "100", "health"));
        assert_eq!(t.entries(TOP)[0].key, "slots");
    }

    #[test]
    fn a_change_touches_only_its_line() {
        let mut t = Table::new("tuning.txt", TEXT);
        assert!(t.set("player", 0, "150"));
        assert!(t.text.contains("hp 150          # health\n"), "{}", t.text);
        assert!(t.set("player", 0, "150000000000000000"));
        assert!(t.text.contains("hp 150000000000000000 # health\n"));
        assert!(t.add("easy", "heal", "30") && t.text.ends_with("damage 0.55\nheal 30\n"));
        assert!(t.remove("player", 1) && !t.text.contains("speed"));
        assert!(t.text.starts_with("# numbers\nslots 6\n\n[player]\n"));
        assert!(!t.set("player", 5, "2"));
    }

    #[test]
    fn numbers_step_by_their_last_place() {
        assert_eq!(step("100", 1, false).as_deref(), Some("101"));
        assert_eq!(step("0.55", -1, false).as_deref(), Some("0.54"));
        assert_eq!(step("0.7", 1, true).as_deref(), Some("1.7"));
        assert_eq!(step("0", -1, false).as_deref(), Some("0"));
        assert_eq!(step("shotgun", 1, false), None);
    }
}
