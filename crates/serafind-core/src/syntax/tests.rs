use std::path::{Path, PathBuf};

use proptest::prelude::*;
use rstest::rstest;

use super::*;
use crate::SearchMode::{self, Apps, Content, Files, Folders};

const HOME: &str = "/home/me";
const CURRENT: &str = "/home/me/Projects";
const EXISTING: [&str; 4] = [
    "/home/me",
    "/home/me/Documents",
    "/home/me/Projects/serafind",
    "/tmp",
];

fn parse_line(line: &str) -> Query {
    let exists = |path: &Path| EXISTING.iter().any(|folder| Path::new(folder) == path);
    let context = ParseContext {
        home: Path::new(HOME),
        current_folder: Path::new(CURRENT),
        folder_exists: &exists,
    };
    parse(line, &context)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|&value| value.to_owned()).collect()
}

fn errors(query: &Query) -> Vec<Option<PartError>> {
    query.parts.iter().map(|part| part.error.clone()).collect()
}

// --- Words ---

#[test]
fn empty_line_asks_for_nothing() {
    for line in ["", "   ", "\t "] {
        let query = parse_line(line);
        assert!(query.is_empty());
        assert!(query.parts.is_empty());
    }
}

#[test]
fn words_phrases_and_exclusions() {
    let query = parse_line(r#"invoice "electricity bill" -draft rep*rt"#);
    assert_eq!(query.words, strings(&["invoice", "electricity bill", "rep*rt"]));
    assert_eq!(query.excluded_words, strings(&["draft"]));
    assert!(!query.has_errors());
}

#[rstest]
#[case("-", "-")]
#[case(r#""-draft""#, "-draft")]
#[case("10:30", "10:30")]
#[case(r"C:\Users", r"C:\Users")]
#[case("d:/photos", "d:/photos")]
#[case("e-mail", "e-mail")]
fn these_are_plain_words(#[case] line: &str, #[case] word: &str) {
    let query = parse_line(line);
    assert_eq!(query.words, strings(&[word]));
    assert_eq!(query.mode, None);
    assert!(!query.has_errors());
}

#[test]
fn spans_point_to_the_typed_parts() {
    let line = r#"  été  ext:pdf "a b"  "#;
    let query = parse_line(line);
    let typed: Vec<&str> = query.parts.iter().map(|part| &line[part.span.clone()]).collect();
    assert_eq!(typed, ["été", "ext:pdf", r#""a b""#]);
}

// --- ext: ---

#[test]
fn extensions_are_lower_case_without_dot() {
    let query = parse_line("ext:PDF,.docx, -ext:tmp");
    assert_eq!(query.extensions, strings(&["pdf", "docx"]));
    assert_eq!(query.excluded_extensions, strings(&["tmp"]));
}

#[rstest]
#[case("ext:")]
#[case("ext:,")]
#[case("ext:.")]
fn extension_filter_needs_a_value(#[case] line: &str) {
    let query = parse_line(line);
    assert_eq!(errors(&query), [Some(PartError::MissingExtension)]);
    assert!(query.is_empty());
}

// --- in: ---

#[rstest]
#[case("in:~", "/home/me", "~")]
#[case("in:~/Documents", "/home/me/Documents", "~/Documents")]
#[case("in:serafind", "/home/me/Projects/serafind", "~/Projects/serafind")]
#[case("in:../Documents", "/home/me/Documents", "~/Documents")]
#[case("in:/tmp", "/tmp", "/tmp")]
#[case(r#"in:"~/Documents""#, "/home/me/Documents", "~/Documents")]
fn start_folder_is_resolved(#[case] line: &str, #[case] path: &str, #[case] shown: &str) {
    let query = parse_line(line);
    assert_eq!(query.start_folder, Some(PathBuf::from(path)));
    assert_eq!(query.parts[0].value, shown);
    assert!(!query.has_errors());
}

#[rstest]
#[case("in:", PartError::MissingFolder)]
#[case("in:~/Nowhere", PartError::FolderNotFound)]
#[case("-in:~", PartError::ExcludedFolder)]
fn bad_start_folders_are_ignored(#[case] line: &str, #[case] error: PartError) {
    let query = parse_line(line);
    assert_eq!(errors(&query), [Some(error)]);
    assert_eq!(query.start_folder, None);
}

#[test]
fn last_start_folder_wins() {
    let query = parse_line("in:~ in:/tmp");
    assert_eq!(query.start_folder, Some(PathBuf::from("/tmp")));
    assert_eq!(errors(&query), [Some(PartError::ReplacedFolder), None]);
}

// --- Modes ---

#[rstest]
#[case("a: fire", Apps, &["fire"])]
#[case("app:fire", Apps, &["fire"])]
#[case("A:fire", Apps, &["fire"])]
#[case("d: projects", Folders, &["projects"])]
#[case("f:invoice", Files, &["invoice"])]
#[case("type:file invoice", Files, &["invoice"])]
#[case("type:folders", Folders, &[])]
#[case("-type:file", Folders, &[])]
#[case("-type:folder", Files, &[])]
fn mode_filters(#[case] line: &str, #[case] mode: SearchMode, #[case] words: &[&str]) {
    let query = parse_line(line);
    assert_eq!(query.mode, Some(mode));
    assert_eq!(query.words, strings(words));
    assert!(!query.has_errors());
}

#[test]
fn content_text_and_name_words_combine() {
    let query = parse_line(r#"report c:"TODO fix" -content:wip"#);
    assert_eq!(query.mode, Some(Content));
    assert_eq!(query.words, strings(&["report"]));
    assert_eq!(query.content, strings(&["TODO fix"]));
    assert_eq!(query.excluded_content, strings(&["wip"]));
    assert!(!query.has_errors());
}

#[test]
fn regular_expressions_for_the_content() {
    let query = parse_line(r#"todo re:"fix(me)?" -re:wip\d"#);
    assert_eq!(query.mode, Some(Content));
    assert_eq!(query.content_patterns, strings(&["fix(me)?"]));
    assert_eq!(query.excluded_content_patterns, strings(&["wip\\d"]));
    assert!(query.content.is_empty());
    assert!(!query.has_errors());
    assert_eq!(parse_line("re:").parts[0].error, Some(PartError::MissingContent));
}

#[test]
fn words_are_the_content_phrase() {
    let query = parse_line("todo fix -draft c:urgent -c:done");
    let (wanted, unwanted) = query.content_texts();
    assert_eq!(wanted, strings(&["todo fix", "urgent"]));
    assert_eq!(unwanted, strings(&["draft", "done"]));
}

#[test]
fn content_filter_needs_a_text() {
    let query = parse_line("c: report");
    assert_eq!(errors(&query), [Some(PartError::MissingContent), None]);
    assert_eq!(query.mode, None);
}

#[rstest]
#[case("-a:")]
#[case("-d:x")]
#[case("-f:")]
#[case("-app:x")]
fn modes_cannot_be_excluded(#[case] line: &str) {
    let query = parse_line(line);
    assert_eq!(errors(&query), [Some(PartError::ExcludedMode)]);
    assert_eq!(query.mode, None);
    assert!(query.words.is_empty());
}

#[test]
fn unknown_type_is_reported() {
    let query = parse_line("type:music");
    assert_eq!(errors(&query), [Some(PartError::UnknownType)]);
    assert_eq!(query.mode, None);
}

#[test]
fn last_mode_wins() {
    let query = parse_line("a: f: report");
    assert_eq!(query.mode, Some(Files));
    assert_eq!(errors(&query), [Some(PartError::ReplacedMode(Files)), None, None]);
}

#[test]
fn replaced_content_filter_gives_no_text() {
    let query = parse_line("c:TODO d:");
    assert_eq!(query.mode, Some(Folders));
    assert!(query.content.is_empty());
}

#[test]
fn same_mode_filters_all_apply() {
    let query = parse_line("c:TODO c:FIXME");
    assert_eq!(query.content, strings(&["TODO", "FIXME"]));
    assert!(!query.has_errors());
}

#[rstest]
#[case("", SearchMode::Files, SearchMode::Files)]
#[case("a: fire", SearchMode::Files, Apps)]
#[case("-a:", SearchMode::Content, SearchMode::Content)]
fn mode_filter_wins_over_the_tab(#[case] line: &str, #[case] tab: SearchMode, #[case] expected: SearchMode) {
    assert_eq!(parse_line(line).effective_mode(tab), expected);
}

// --- Unknown filters ---

#[test]
fn unknown_filters_are_searched_as_words() {
    let query = parse_line(r#"size:big -tag:"old one""#);
    assert_eq!(query.words, strings(&["size:big"]));
    assert_eq!(query.excluded_words, strings(&["tag:old one"]));
    assert!(query.parts.iter().all(Part::is_used));
    assert!(matches!(&query.parts[0].error, Some(PartError::UnknownFilter(key)) if key == "size"));
}

// --- Snapshot of a full line ---

#[test]
fn full_line_snapshot() {
    let mut query = parse_line(r#"invoice ext:pdf,docx in:~/Documents -draft c:"total due" a: unknown:x"#);
    // The same snapshot on every system: Windows writes the folder with \.
    query.start_folder = query
        .start_folder
        .map(|folder| PathBuf::from(folder.to_string_lossy().replace('\\', "/")));
    insta::assert_debug_snapshot!(query);
}

// --- Properties ---

proptest! {
    #[test]
    fn never_panics_and_spans_cover_the_text(line in "\\PC{0,60}") {
        let query = parse_line(&line);
        for part in &query.parts {
            prop_assert_eq!(&line[part.span.clone()], part.text.as_str());
            prop_assert!(!part.text.trim().is_empty());
        }
    }

    #[test]
    fn parts_never_overlap(line in "[a-z:\"\\- ]{0,40}") {
        let query = parse_line(&line);
        for pair in query.parts.windows(2) {
            prop_assert!(pair[0].span.end < pair[1].span.start);
        }
    }

    #[test]
    fn plain_words_are_kept_as_typed(words in prop::collection::vec("[a-zA-Zéè0-9_.]{1,10}", 0..6)) {
        let query = parse_line(&words.join(" "));
        prop_assert_eq!(query.words, words);
    }
}
