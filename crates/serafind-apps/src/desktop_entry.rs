//! The `.desktop` file format (freedesktop.org Desktop Entry Specification): groups of
//! `key=value` lines, localized keys (`Name[fr]`) and the `Exec` command line.

use std::collections::HashMap;

/// The `[Desktop Entry]` group of a `.desktop` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DesktopEntry {
    values: HashMap<String, String>,
}

impl DesktopEntry {
    /// Reads the `[Desktop Entry]` group; other groups (desktop actions) are ignored.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut values = HashMap::new();
        let mut in_entry = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_entry {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                // The first value of a key wins, as in other readers.
                values
                    .entry(key.trim().to_owned())
                    .or_insert_with(|| unescape(value.trim()));
            }
        }
        Self { values }
    }

    /// A value, as written.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// A value in the first of `locales` it is translated to (`Name[fr_FR]`, `Name[fr]`...), or
    /// the untranslated one.
    #[must_use]
    pub fn localized(&self, key: &str, locales: &[String]) -> Option<&str> {
        locales
            .iter()
            .find_map(|locale| self.get(&format!("{key}[{locale}]")))
            .or_else(|| self.get(key))
    }

    /// A boolean value (`true` / `false`); false when missing.
    #[must_use]
    pub fn flag(&self, key: &str) -> bool {
        self.get(key) == Some("true")
    }

    /// A list value (`GNOME;KDE;`).
    #[must_use]
    pub fn list(&self, key: &str) -> Vec<&str> {
        self.get(key)
            .map(|value| {
                value
                    .split(';')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The escapes of string values: `\s`, `\n`, `\t`, `\r`, `\\`.
fn unescape(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => text.push(' '),
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('r') => text.push('\r'),
            // `\\` becomes `\`; other escapes (`\"` and `\$` for Exec quoting, `\;` in lists)
            // are kept for the readers of those values.
            Some('\\') | None => text.push('\\'),
            Some(other) => {
                text.push('\\');
                text.push(other);
            }
        }
    }
    text
}

/// What the `%` field codes of an `Exec` line are replaced with. No file is passed: the
/// application is started on its own.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecContext<'a> {
    /// The `Icon` value (`%i`).
    pub icon: Option<&'a str>,
    /// The translated name (`%c`).
    pub name: &'a str,
    /// The `.desktop` file (`%k`).
    pub file: &'a str,
}

/// The arguments of an `Exec` line: quoted arguments understood, field codes replaced or
/// removed. `None` when the line is malformed (unclosed quote) or empty.
#[must_use]
pub fn exec_arguments(exec: &str, context: &ExecContext<'_>) -> Option<Vec<String>> {
    let mut arguments = Vec::new();
    for word in split_quoted(exec)? {
        match word.as_str() {
            // Files and URLs: none are given. Deprecated codes are removed too.
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            "%i" => {
                if let Some(icon) = context.icon {
                    arguments.push("--icon".to_owned());
                    arguments.push(icon.to_owned());
                }
            }
            _ => arguments.push(expand_codes(&word, context)),
        }
    }
    (!arguments.is_empty()).then_some(arguments)
}

/// Field codes inside a word (`--name=%c`), and `%%`.
fn expand_codes(word: &str, context: &ExecContext<'_>) -> String {
    let mut text = String::with_capacity(word.len());
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => text.push('%'),
            Some('c') => text.push_str(context.name),
            Some('k') => text.push_str(context.file),
            // Other codes inside a word stand for files or are deprecated: removed.
            Some(_) | None => {}
        }
    }
    text
}

/// Splits on spaces outside double quotes; inside quotes `\"`, `` \` ``, `\$` and `\\` are
/// escapes.
fn split_quoted(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                in_word = true;
            }
            '\\' if quoted => match chars.next() {
                Some(escaped @ ('"' | '`' | '$' | '\\')) => word.push(escaped),
                Some(other) => {
                    word.push('\\');
                    word.push(other);
                }
                None => return None,
            },
            ' ' | '\t' if !quoted => {
                if in_word {
                    words.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            _ => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if in_word {
        words.push(word);
    }
    Some(words)
}

/// The locales to try for translated values, best first, from a locale name such as
/// `fr_FR.UTF-8@euro`: `fr_FR@euro`, `fr_FR`, `fr@euro`, `fr`.
#[must_use]
pub fn locale_keys(locale: &str) -> Vec<String> {
    let (rest, modifier) = match locale.split_once('@') {
        Some((rest, modifier)) => (rest, Some(modifier)),
        None => (locale, None),
    };
    let rest = rest.split('.').next().unwrap_or(rest);
    if rest.is_empty() || rest == "C" || rest == "POSIX" {
        return Vec::new();
    }
    let (language, country) = match rest.split_once('_') {
        Some((language, country)) => (language, Some(country)),
        None => (rest, None),
    };
    let mut keys = Vec::new();
    if let (Some(country), Some(modifier)) = (country, modifier) {
        keys.push(format!("{language}_{country}@{modifier}"));
    }
    if let Some(country) = country {
        keys.push(format!("{language}_{country}"));
    }
    if let Some(modifier) = modifier {
        keys.push(format!("{language}@{modifier}"));
    }
    keys.push(language.to_owned());
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const FIREFOX: &str = "[Desktop Entry]\n\
        Name=Firefox\n\
        Name[fr]=Firefox (fr)\n\
        GenericName=Web Browser\n\
        Exec=firefox %u\n\
        Icon=firefox\n\
        Categories=Network;WebBrowser;\n\
        \n\
        [Desktop Action new-private-window]\n\
        Name=New Private Window\n\
        Exec=firefox --private-window %u\n";

    #[test]
    fn reads_the_entry_group_only() {
        let entry = DesktopEntry::parse(FIREFOX);
        assert_eq!(entry.get("Name"), Some("Firefox"));
        assert_eq!(entry.get("Exec"), Some("firefox %u"));
        assert_eq!(entry.list("Categories"), ["Network", "WebBrowser"]);
        assert!(!entry.flag("NoDisplay"));
    }

    #[test]
    fn translated_values() {
        let entry = DesktopEntry::parse(FIREFOX);
        assert_eq!(
            entry.localized("Name", &locale_keys("fr_FR.UTF-8")),
            Some("Firefox (fr)")
        );
        assert_eq!(
            entry.localized("Name", &locale_keys("en_GB.UTF-8")),
            Some("Firefox")
        );
        assert_eq!(entry.localized("Name", &[]), Some("Firefox"));
    }

    #[rstest]
    #[case("fr_FR.UTF-8@euro", &["fr_FR@euro", "fr_FR", "fr@euro", "fr"])]
    #[case("en_GB.UTF-8", &["en_GB", "en"])]
    #[case("de", &["de"])]
    #[case("C.UTF-8", &[])]
    #[case("", &[])]
    fn locales(#[case] locale: &str, #[case] keys: &[&str]) {
        assert_eq!(locale_keys(locale), keys);
    }

    #[rstest]
    #[case("firefox %u", &["firefox"])]
    #[case("gimp-2.10 %U", &["gimp-2.10"])]
    #[case("\"/opt/My App/run\" --flag %F", &["/opt/My App/run", "--flag"])]
    #[case("sh -c \"echo \\\"hi\\\" \\$HOME\"", &["sh", "-c", "echo \"hi\" $HOME"])]
    #[case("app --name=%c --file %k 100%%", &["app", "--name=Shown", "--file", "/a.desktop", "100%"])]
    #[case("app %i", &["app", "--icon", "app-icon"])]
    #[case("flatpak run --branch=stable org.gnome.Maps @@u %U @@", &["flatpak", "run", "--branch=stable", "org.gnome.Maps", "@@u", "@@"])]
    fn exec_lines(#[case] exec: &str, #[case] expected: &[&str]) {
        let context = ExecContext {
            icon: Some("app-icon"),
            name: "Shown",
            file: "/a.desktop",
        };
        assert_eq!(exec_arguments(exec, &context).unwrap(), expected);
    }

    #[test]
    fn bad_exec_lines() {
        let context = ExecContext::default();
        assert_eq!(exec_arguments("app \"unclosed", &context), None);
        assert_eq!(exec_arguments("  %U ", &context), None);
    }

    #[test]
    fn escapes_in_values() {
        let entry = DesktopEntry::parse("[Desktop Entry]\nComment=a\\sb\\\\c\nExec=\"/x y/z\"\n");
        assert_eq!(entry.get("Comment"), Some("a b\\c"));
        assert_eq!(entry.get("Exec"), Some("\"/x y/z\""));
    }
}
