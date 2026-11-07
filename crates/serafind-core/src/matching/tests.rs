use std::path::Path;

use proptest::prelude::*;
use rstest::rstest;

use super::*;
use crate::{ParseContext, parse};

fn query(line: &str) -> Query {
    let context = ParseContext {
        home: Path::new("/home/me"),
        current_folder: Path::new("/home/me"),
        folder_exists: &|_| true,
    };
    parse(line, &context)
}

fn matcher(line: &str, mode: SearchMode) -> Matcher {
    Matcher::new(&query(line), mode)
}

fn file(path: &str) -> Candidate<'_> {
    entry(path, EntryKind::File)
}

fn folder(path: &str) -> Candidate<'_> {
    entry(path, EntryKind::Folder)
}

fn app(name: &str) -> Candidate<'_> {
    Candidate {
        name,
        folded_name: None,
        depth: 0,
        kind: EntryKind::App,
        id: 0,
    }
}

fn entry(path: &str, kind: EntryKind) -> Candidate<'_> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let depth = u16::try_from(path.matches('/').count()).unwrap();
    Candidate {
        name,
        folded_name: None,
        depth,
        kind,
        id: 0,
    }
}

/// Gives each candidate its position as id, to find its path back after ranking.
fn numbered<'a>(candidates: impl IntoIterator<Item = Candidate<'a>>) -> Vec<Candidate<'a>> {
    candidates
        .into_iter()
        .zip(0..)
        .map(|(candidate, id)| Candidate { id, ..candidate })
        .collect()
}

fn names<'a>(ranked: &Ranked<'a>) -> Vec<&'a str> {
    ranked
        .entries
        .iter()
        .map(|(_, candidate)| candidate.name)
        .collect()
}

// --- Strict matching ---

#[rstest]
#[case("invoice", "invoice-2026.pdf", true)]
#[case("invoice 2026", "2026-invoice.pdf", true)]
#[case("invoice 2027", "invoice-2026.pdf", false)]
#[case("facture", "Facturé EDF.pdf", true)]
#[case("Facture", "facture.pdf", false)]
#[case(r#""final report""#, "the final report.odt", true)]
#[case(r#""final report""#, "report final.odt", false)]
#[case("rep*rt", "report", true)]
#[case("rep*rt", "report.pdf", true)]
#[case("rep*rt", "my-report-v2", true)]
#[case("^rep*rt$", "report.pdf", false)]
#[case("^rep*rt$", "report", true)]
#[case("*.pdf", "INVOICE.PDF", true)]
#[case("*.pdf", "a.pdf.txt", true)]
#[case(".pdf$", "a.pdf.txt", false)]
#[case(".pdf$", "a.pdf", true)]
#[case("^inv", "invoice", true)]
#[case("^inv", "my-invoice", false)]
#[case("^", "a^b", true)]
#[case("$Recycle", "$Recycle.Bin", true)]
#[case("invoice -draft", "invoice-draft.pdf", false)]
#[case("invoice -draft", "invoice.pdf", true)]
#[case("ext:pdf", "invoice.PDF", true)]
#[case("ext:pdf", "invoice.pdf.txt", false)]
#[case("ext:pdf,odt", "report.odt", true)]
#[case("-ext:tmp", "report.tmp", false)]
#[case("ext:bashrc", ".bashrc", false)]
#[case("ext:gz", "archive.tar.gz", true)]
#[case("", "anything", true)]
fn strict_matching_of_files(#[case] line: &str, #[case] name: &str, #[case] expected: bool) {
    let mut matcher = matcher(line, SearchMode::All);
    assert_eq!(
        matcher.strict(&file(name)).is_some(),
        expected,
        "{line:?} on {name:?}"
    );
}

#[rstest]
#[case(SearchMode::All, &["Firefox", "firefox.desktop", "firefox-profiles"])]
#[case(SearchMode::Apps, &["Firefox"])]
#[case(SearchMode::Folders, &["firefox-profiles"])]
#[case(SearchMode::Files, &["firefox.desktop"])]
#[case(SearchMode::Content, &["firefox.desktop"])]
fn mode_keeps_the_right_kinds(#[case] mode: SearchMode, #[case] expected: &[&str]) {
    let candidates = [
        app("Firefox"),
        folder("/home/me/firefox-profiles"),
        file("/home/me/firefox.desktop"),
    ];
    let ranked = matcher("firefox", mode).best(candidates, 10);
    assert_eq!(names(&ranked), expected);
}

#[test]
fn ready_folded_names_give_the_same_results() {
    let folded = crate::text::fold("Facturé-EDF.pdf", crate::text::CaseMatching::Ignore);
    for line in ["facture edf", "fac*edf*", "-edf", "EDF", "factur"] {
        let plain = file("Facturé-EDF.pdf");
        let ready = Candidate {
            folded_name: Some(&folded),
            ..plain
        };
        let mut matcher = matcher(line, SearchMode::All);
        assert_eq!(matcher.strict(&plain), matcher.strict(&ready), "{line:?}");
    }
}

#[test]
fn apps_have_no_extension() {
    let mut matcher = matcher("ext:desktop", SearchMode::All);
    assert!(matcher.strict(&app("firefox.desktop")).is_none());
}

// --- Ranking ---

#[rstest]
#[case("report", "report.pdf", MatchRank::Exact)]
#[case("report", "Report", MatchRank::Exact)]
#[case("final report", "final report.odt", MatchRank::Exact)]
#[case("report", "reports-2026", MatchRank::NamePrefix)]
#[case("report 2026", "report-final-2026", MatchRank::NamePrefix)]
#[case("report", "final-report", MatchRank::WordPrefix)]
#[case("Report", "myReport.txt", MatchRank::WordPrefix)]
#[case("2026", "invoice2026", MatchRank::WordPrefix)]
#[case("port", "report", MatchRank::Contains)]
#[case("rep*rt", "report", MatchRank::Contains)]
#[case("ext:pdf", "report.pdf", MatchRank::Contains)]
fn match_ranks(#[case] line: &str, #[case] name: &str, #[case] rank: MatchRank) {
    let score = matcher(line, SearchMode::All)
        .strict(&file(name))
        .expect("matches");
    assert_eq!(score.match_rank(), rank);
}

#[test]
fn ranking_order() {
    let paths = [
        "/home/me/a/b/c/support.txt",
        "/home/me/final-report.pdf",
        "/home/me/deep/down/report.pdf",
        "/home/me/report.pdf",
        "/home/me/reports-2026.ods",
        "/home/me/Reports",
        "Report Designer",
    ];
    let candidates = numbered(paths.iter().map(|&path| match path {
        "/home/me/Reports" => folder(path),
        "Report Designer" => app(path),
        _ => file(path),
    }));
    let ranked = matcher("report", SearchMode::All).best(candidates, 10);
    let paths: Vec<&str> = ranked
        .entries
        .iter()
        .map(|(_, candidate)| paths[candidate.id as usize])
        .collect();
    insta::assert_debug_snapshot!(paths);
    assert!(!ranked.is_fuzzy);
}

#[test]
fn limit_keeps_the_best_and_counts_all() {
    let paths: Vec<String> = (0..100)
        .map(|index| format!("/home/me/notes-{index:03}.txt"))
        .collect();
    let mut candidates: Vec<Candidate<'_>> = paths.iter().map(|path| file(path)).collect();
    candidates.push(file("/home/me/notes.txt"));
    let ranked = matcher("notes", SearchMode::Files).best(candidates, 3);
    assert_eq!(names(&ranked), ["notes.txt", "notes-000.txt", "notes-001.txt"]);
    assert_eq!(ranked.total, 101);
}

#[test]
fn zero_limit_returns_nothing() {
    let ranked = matcher("a", SearchMode::All).best([file("a")], 0);
    assert!(ranked.entries.is_empty());
    assert_eq!(ranked.total, 1);
}

// --- Fuzzy fallback ---

#[test]
fn fuzzy_fallback_when_nothing_matches() {
    let candidates = [file("/home/me/facture.pdf"), file("/home/me/photo.jpg")];
    let ranked = matcher("fctr", SearchMode::All).best(candidates, 10);
    assert!(ranked.is_fuzzy);
    assert_eq!(names(&ranked), ["facture.pdf"]);
    assert_eq!(ranked.entries[0].0.match_rank(), MatchRank::Fuzzy);
}

#[test]
fn no_fuzzy_results_once_something_matches_strictly() {
    let candidates = [file("/home/me/fctr-notes.txt"), file("/home/me/facture.pdf")];
    let ranked = matcher("fctr", SearchMode::All).best(candidates, 10);
    assert!(!ranked.is_fuzzy);
    assert_eq!(names(&ranked), ["fctr-notes.txt"]);
}

#[test]
fn fuzzy_fallback_ignores_accents_and_keeps_filters() {
    let candidates = [
        file("/home/me/facturé-edf.pdf"),
        file("/home/me/facture-edf.odt"),
        file("/home/me/factures-draft.pdf"),
    ];
    let ranked = matcher("fctredf ext:pdf -draft", SearchMode::All).best(candidates, 10);
    assert!(ranked.is_fuzzy);
    assert_eq!(names(&ranked), ["facturé-edf.pdf"]);
}

#[test]
fn fuzzy_ranks_closer_matches_first() {
    let candidates = [file("/home/me/fxaxcxtxuxrxe.txt"), file("/home/me/facture.txt")];
    let ranked = matcher("fcture", SearchMode::All).best(candidates, 10);
    assert_eq!(names(&ranked), ["facture.txt", "fxaxcxtxuxrxe.txt"]);
}

// --- Highlights ---

#[rstest]
#[case("fact", "facture.pdf", false, &[0, 1, 2, 3])]
#[case("ture 2026", "facture-2026.pdf", false, &[3, 4, 5, 6, 8, 9, 10, 11])]
#[case("ete", "L'Été.txt", false, &[2, 3, 4])]
#[case("rep*rt", "report", false, &[])]
#[case("fctr", "facture", true, &[0, 2, 3, 5])]
fn highlights(#[case] line: &str, #[case] name: &str, #[case] fuzzy: bool, #[case] expected: &[usize]) {
    let mut matcher = matcher(line, SearchMode::All);
    assert_eq!(matcher.highlights(&file(name), fuzzy), expected);
}

// --- Wildcards ---

#[rstest]
#[case("*", "", true)]
#[case("*", "abc", true)]
#[case("a*c", "abbbc", true)]
#[case("a*c", "xabcd", true)]
#[case("^a*c$", "abcd", false)]
#[case("^a*c$", "abbbc", true)]
#[case("a?c", "abc", true)]
#[case("a?c", "ac", false)]
#[case("^a?c$", "abcc", false)]
#[case("*a*a*", "banana", true)]
#[case("^é*", "Été", true)]
#[case("^b", "abc", false)]
#[case("c$", "abc", true)]
fn wildcards(#[case] pattern: &str, #[case] name: &str, #[case] expected: bool) {
    assert_eq!(Glob::new(pattern).matches(name), expected);
}

proptest! {
    #[test]
    fn star_matches_everything(name in "\\PC{0,20}") {
        prop_assert!(Glob::new("*").matches(&name));
    }

    #[test]
    fn a_name_matches_itself_as_a_pattern(name in "[a-z0-9 .-]{0,20}") {
        prop_assert!(Glob::new(&name).matches(&name));
    }

    #[test]
    fn strict_and_fuzzy_never_panic(line in "\\PC{0,30}", name in "\\PC{0,30}") {
        let mut matcher = matcher(&line, SearchMode::All);
        let candidate = file(&name);
        let _ = matcher.strict(&candidate);
        let _ = matcher.fuzzy(&candidate);
    }

    #[test]
    fn best_is_sorted_and_within_limit(
        names in prop::collection::vec("[a-c]{1,4}", 0..40),
        limit in 0usize..10,
    ) {
        let candidates: Vec<Candidate<'_>> = names.iter().map(|name| file(name)).collect();
        let ranked = matcher("a", SearchMode::All).best(candidates, limit);
        prop_assert!(ranked.entries.len() <= limit);
        prop_assert!(ranked.entries.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    }
}
