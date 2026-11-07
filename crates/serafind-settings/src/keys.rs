//! Keyboard shortcuts as written in `config.toml`: modifiers and a key joined with `+`
//! (`Super+F`, `Ctrl+Alt+Space`). Case does not matter.

use std::fmt;

/// A shortcut understood.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one flag per modifier key")]
pub struct Keys {
    /// Ctrl.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Super: the Windows / logo key (Meta on KDE).
    pub logo: bool,
    /// The key.
    pub key: Key,
}

/// The key of a shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A letter, upper case.
    Letter(char),
    /// A digit of the main keyboard.
    Digit(char),
    /// F1 to F24.
    Function(u8),
    /// Space bar.
    Space,
    /// Enter.
    Enter,
    /// Tab.
    Tab,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// The key above Tab (`` ` `` on many layouts).
    Grave,
}

impl Keys {
    /// Reads `Super+F`, `ctrl+alt+space`, `F12`...
    ///
    /// # Errors
    /// A message saying what is wrong: unknown modifier or key, no key, or a key that would be
    /// taken from normal typing (a letter, digit or space without Ctrl, Alt or Super).
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let Some((key, modifiers)) = parts.split_last() else {
            return Err("is empty".to_owned());
        };
        let mut keys = Self {
            ctrl: false,
            alt: false,
            shift: false,
            logo: false,
            key: Key::parse(key)?,
        };
        for modifier in modifiers {
            let slot = match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut keys.ctrl,
                "alt" | "option" => &mut keys.alt,
                "shift" => &mut keys.shift,
                "super" | "win" | "windows" | "meta" | "logo" | "cmd" | "command" => &mut keys.logo,
                "" => return Err(format!("{text:?} has an empty part")),
                other => {
                    return Err(format!(
                        "unknown modifier {other:?} (use Ctrl, Alt, Shift or Super)"
                    ));
                }
            };
            *slot = true;
        }
        let typing = matches!(
            keys.key,
            Key::Letter(_) | Key::Digit(_) | Key::Space | Key::Enter | Key::Tab
        );
        if typing && !(keys.ctrl || keys.alt || keys.logo) {
            return Err(format!(
                "{text:?} would be taken from normal typing: add Ctrl, Alt or Super"
            ));
        }
        Ok(keys)
    }

    /// The shortcut in the XDG shortcuts format the desktop portals use (`LOGO+f`).
    #[must_use]
    pub fn to_xdg(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (on, name) in [
            (self.ctrl, "CTRL"),
            (self.alt, "ALT"),
            (self.shift, "SHIFT"),
            (self.logo, "LOGO"),
        ] {
            if on {
                parts.push(name.to_owned());
            }
        }
        parts.push(match self.key {
            Key::Letter(letter) => letter.to_ascii_lowercase().to_string(),
            Key::Digit(digit) => digit.to_string(),
            Key::Function(number) => format!("F{number}"),
            Key::Space => "space".to_owned(),
            Key::Enter => "Return".to_owned(),
            Key::Tab => "Tab".to_owned(),
            Key::Insert => "Insert".to_owned(),
            Key::Delete => "Delete".to_owned(),
            Key::Home => "Home".to_owned(),
            Key::End => "End".to_owned(),
            Key::PageUp => "Page_Up".to_owned(),
            Key::PageDown => "Page_Down".to_owned(),
            Key::Grave => "grave".to_owned(),
        });
        parts.join("+")
    }
}

impl Key {
    fn parse(text: &str) -> Result<Self, String> {
        let lower = text.to_ascii_lowercase();
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if c.is_ascii_alphabetic() {
                return Ok(Self::Letter(c.to_ascii_uppercase()));
            }
            if c.is_ascii_digit() {
                return Ok(Self::Digit(c));
            }
            if c == '`' {
                return Ok(Self::Grave);
            }
        }
        if let Some(number) = lower
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            && (1..=24).contains(&number)
        {
            return Ok(Self::Function(number));
        }
        Ok(match lower.as_str() {
            "space" => Self::Space,
            "enter" | "return" => Self::Enter,
            "tab" => Self::Tab,
            "insert" | "ins" => Self::Insert,
            "delete" | "del" => Self::Delete,
            "home" => Self::Home,
            "end" => Self::End,
            "pageup" | "page_up" | "pgup" => Self::PageUp,
            "pagedown" | "page_down" | "pgdn" => Self::PageDown,
            "grave" | "backquote" => Self::Grave,
            "" => return Err("has no key after the last +".to_owned()),
            other => return Err(format!("unknown key {other:?}")),
        })
    }
}

impl fmt::Display for Keys {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.logo, "Super"),
        ] {
            if on {
                write!(formatter, "{name}+")?;
            }
        }
        match self.key {
            Key::Letter(letter) | Key::Digit(letter) => write!(formatter, "{letter}"),
            Key::Function(number) => write!(formatter, "F{number}"),
            Key::Grave => formatter.write_str("`"),
            other => write!(formatter, "{other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("Super+F", "LOGO+f", "Super+F")]
    #[case("super+f", "LOGO+f", "Super+F")]
    #[case("Ctrl+Alt+Space", "CTRL+ALT+space", "Ctrl+Alt+Space")]
    #[case("alt + shift + space", "ALT+SHIFT+space", "Alt+Shift+Space")]
    #[case("Win+1", "LOGO+1", "Super+1")]
    #[case("F12", "F12", "F12")]
    #[case("Ctrl+PageDown", "CTRL+Page_Down", "Ctrl+PageDown")]
    #[case("Meta+`", "LOGO+grave", "Super+`")]
    fn shortcuts(#[case] text: &str, #[case] xdg: &str, #[case] shown: &str) {
        let keys = Keys::parse(text).unwrap();
        assert_eq!(keys.to_xdg(), xdg);
        assert_eq!(keys.to_string(), shown);
    }

    #[rstest]
    #[case("", "no key")]
    #[case("Super+", "no key")]
    #[case("Hyper+F", "unknown modifier")]
    #[case("Super+Banana", "unknown key")]
    #[case("F", "normal typing")]
    #[case("Shift+F", "normal typing")]
    #[case("Shift+Space", "normal typing")]
    #[case("F25", "unknown key")]
    #[case("Ctrl++F", "empty part")]
    fn bad_shortcuts(#[case] text: &str, #[case] problem: &str) {
        let error = Keys::parse(text).unwrap_err();
        assert!(error.contains(problem), "{text:?}: {error}");
    }
}
