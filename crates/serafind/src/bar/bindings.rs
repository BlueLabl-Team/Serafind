//! The keys of the bar, from `[keys]` in `config.toml`, as egui key presses.

use eframe::egui::{Key, Modifiers};
use serafind_settings::{BarKeys, Keys, Modifiers as HeldKeys};

/// What a key of the bar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Open,
    Reveal,
    CopyPath,
    CopyName,
    Terminal,
    Close,
    NextResult,
    PreviousResult,
    NextPage,
    PreviousPage,
    /// Before `NextMode`: wins when both have the same key and a folder is proposed.
    Complete,
    NextMode,
    PreviousMode,
}

/// The keys of the bar, longest combinations first: egui ignores an extra Shift or Alt, so
/// `Ctrl+Shift+C` must be tried before `Ctrl+C`.
#[derive(Debug, Clone)]
pub struct Bindings {
    keys: Vec<(Modifiers, Key, Command)>,
    /// With a digit 1 to 5: choose a mode.
    pub mode_with_digit: Modifiers,
    /// With a digit 1 to 9: open that result.
    pub open_with_digit: Modifiers,
}

impl Bindings {
    /// The keys of `settings`; a key that cannot be read is left out (`config.toml` is refused
    /// then, so this only happens with the defaults, which are all valid).
    #[must_use]
    pub fn new(settings: &BarKeys) -> Self {
        let commands = [
            (&settings.open, Command::Open),
            (&settings.reveal, Command::Reveal),
            (&settings.copy_path, Command::CopyPath),
            (&settings.copy_name, Command::CopyName),
            (&settings.terminal, Command::Terminal),
            (&settings.close, Command::Close),
            (&settings.next_result, Command::NextResult),
            (&settings.previous_result, Command::PreviousResult),
            (&settings.next_page, Command::NextPage),
            (&settings.previous_page, Command::PreviousPage),
            (&settings.complete, Command::Complete),
            (&settings.next_mode, Command::NextMode),
            (&settings.previous_mode, Command::PreviousMode),
        ];
        let mut keys: Vec<(Modifiers, Key, Command)> = commands
            .into_iter()
            .filter_map(|(text, command)| {
                let keys = Keys::parse_bar(text).ok()?;
                Some((modifiers(keys.modifiers()), key(&keys)?, command))
            })
            .collect();
        // Stable: among equal lengths, the order above stays (Complete before NextMode).
        keys.sort_by_key(|(modifiers, _, _)| std::cmp::Reverse(count(*modifiers)));
        let digit = |text: &str, default: Modifiers| HeldKeys::parse(text).map_or(default, modifiers);
        Self {
            keys,
            mode_with_digit: digit(&settings.mode_with_digit, Modifiers::COMMAND),
            open_with_digit: digit(&settings.open_with_digit, Modifiers::ALT),
        }
    }

    /// The keys, in the order to try them.
    pub fn keys(&self) -> impl Iterator<Item = (Modifiers, Key, Command)> + '_ {
        self.keys.iter().copied()
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self::new(&BarKeys::default())
    }
}

/// Ctrl is egui's "command" on Linux and Windows.
fn modifiers(held: HeldKeys) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if held.ctrl {
        modifiers |= Modifiers::COMMAND;
    }
    if held.alt {
        modifiers |= Modifiers::ALT;
    }
    if held.shift {
        modifiers |= Modifiers::SHIFT;
    }
    modifiers
}

fn count(modifiers: Modifiers) -> usize {
    [modifiers.command, modifiers.alt, modifiers.shift]
        .into_iter()
        .filter(|&on| on)
        .count()
}

fn key(keys: &Keys) -> Option<Key> {
    use serafind_settings::Key as K;
    Some(match keys.key {
        K::Letter(letter) => Key::from_name(&letter.to_string())?,
        K::Digit(digit) => Key::from_name(&digit.to_string())?,
        K::Function(number) => Key::from_name(&format!("F{number}"))?,
        K::Space => Key::Space,
        K::Enter => Key::Enter,
        K::Tab => Key::Tab,
        K::Insert => Key::Insert,
        K::Delete => Key::Delete,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,
        K::Grave => Key::Backtick,
        K::Escape => Key::Escape,
        K::Backspace => Key::Backspace,
        K::Up => Key::ArrowUp,
        K::Down => Key::ArrowDown,
        K::Left => Key::ArrowLeft,
        K::Right => Key::ArrowRight,
    })
}

/// The digit keys 1 to 9.
pub const DIGITS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_read_longest_first() {
        let bindings = Bindings::default();
        let keys: Vec<_> = bindings.keys().collect();
        assert_eq!(keys.len(), 13);
        assert_eq!(
            keys[0],
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::C, Command::CopyName)
        );
        let position = |command| keys.iter().position(|(_, _, c)| *c == command).unwrap();
        assert!(position(Command::Reveal) < position(Command::Open));
        assert!(position(Command::PreviousMode) < position(Command::NextMode));
        assert!(position(Command::Complete) < position(Command::NextMode));
        assert_eq!(bindings.mode_with_digit, Modifiers::COMMAND);
        assert_eq!(bindings.open_with_digit, Modifiers::ALT);
    }

    #[test]
    fn keys_can_be_changed() {
        let settings = BarKeys {
            terminal: "Alt+F12".to_owned(),
            open_with_digit: "Ctrl+Alt".to_owned(),
            ..BarKeys::default()
        };
        let bindings = Bindings::new(&settings);
        assert!(
            bindings
                .keys()
                .any(|binding| binding == (Modifiers::ALT, Key::F12, Command::Terminal))
        );
        assert_eq!(bindings.open_with_digit, Modifiers::COMMAND | Modifiers::ALT);
    }
}
