//! Minimal reader for Valve's text KeyValues format (`libraryfolders.vdf`,
//! `appmanifest_*.acf`).

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// Case-insensitive lookup, as Steam treats keys.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(v) => v.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Value::Str(_) => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }
    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(v) => v,
            Value::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            c if c.is_whitespace() => {}
            '{' => out.push(Tok::Open),
            '}' => out.push(Tok::Close),
            '/' if it.peek() == Some(&'/') => {
                for c in it.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                let mut s = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '"' => break,
                        '\\' => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => {}
                        },
                        other => s.push(other),
                    }
                }
                out.push(Tok::Str(s));
            }
            other => {
                let mut s = String::from(other);
                while let Some(&c) = it.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    it.next();
                }
                out.push(Tok::Str(s));
            }
        }
    }
    out
}

fn parse_obj(toks: &[Tok], pos: &mut usize, top: bool) -> Value {
    let mut items = Vec::new();
    while *pos < toks.len() {
        match &toks[*pos] {
            Tok::Close => {
                *pos += 1;
                if !top {
                    break;
                }
            }
            Tok::Open => {
                // Brace without a key: skip it, stay lenient.
                *pos += 1;
            }
            Tok::Str(key) => {
                *pos += 1;
                match toks.get(*pos) {
                    Some(Tok::Str(v)) => {
                        items.push((key.clone(), Value::Str(v.clone())));
                        *pos += 1;
                    }
                    Some(Tok::Open) => {
                        *pos += 1;
                        items.push((key.clone(), parse_obj(toks, pos, false)));
                    }
                    _ => {}
                }
            }
        }
    }
    Value::Obj(items)
}

/// Parses a whole file. Malformed input gives whatever could be read.
pub fn parse(text: &str) -> Value {
    let toks = tokenize(text);
    let mut pos = 0;
    parse_obj(&toks, &mut pos, true)
}

/// Library folders listed in `libraryfolders.vdf`. Handles the current layout
/// (`"0" { "path" "..." }`) and the old one (`"1" "D:\\SteamLibrary"`).
pub fn library_paths(text: &str) -> Vec<PathBuf> {
    let root = parse(text);
    let Some(folders) = root.get("libraryfolders") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, value) in folders.entries() {
        match value {
            Value::Obj(_) => {
                if let Some(p) = value.get("path").and_then(Value::as_str) {
                    out.push(PathBuf::from(p));
                }
            }
            Value::Str(s) => {
                if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
                    out.push(PathBuf::from(s));
                }
            }
        }
    }
    out
}

/// `installdir` of an `appmanifest_<id>.acf`.
pub fn acf_installdir(text: &str) -> Option<String> {
    parse(text).get("AppState")?.get("installdir")?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEW: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"1234"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps" { "311210" "999" "545990" "1" }
	}
}
"#;

    #[test]
    fn current_library_format() {
        assert_eq!(
            library_paths(NEW),
            vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\SteamLibrary")]
        );
    }

    #[test]
    fn old_library_format() {
        let old = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"123\"\n\t\"ContentStatsID\"\t\"-5\"\n\t\"1\"\t\"E:\\\\Games\\\\Steam\"\n}\n";
        assert_eq!(library_paths(old), vec![PathBuf::from(r"E:\Games\Steam")]);
    }

    #[test]
    fn acf() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\"545990\"\n\t// comment\n\t\"installdir\"\t\"Call of Duty Black Ops III 545990\"\n\t\"UserConfig\" { \"x\" \"y\" }\n}";
        assert_eq!(acf_installdir(acf).as_deref(), Some("Call of Duty Black Ops III 545990"));
    }

    #[test]
    fn garbage_does_not_panic() {
        assert!(library_paths("}}}{ \"a").is_empty());
        assert!(acf_installdir("").is_none());
        assert!(library_paths("\"libraryfolders\" { \"0\" { \"path\"").is_empty());
    }
}
