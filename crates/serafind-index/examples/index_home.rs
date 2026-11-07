//! Indexes the home directory with the default exclusions and runs a few searches.
//!
//! `cargo run --release -p serafind-index --example index_home -- [query...]`

use std::path::{Path, PathBuf};
use std::time::Instant;

use serafind_core::{ParseContext, SearchMode, parse};
use serafind_index::{Exclusions, build};

fn main() {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .expect("a home directory");
    let (index, report) = build(std::slice::from_ref(&home), &Exclusions::defaults(&home));
    println!(
        "{} entries in {:.0?} ({} unreadable), about {} KiB",
        report.entries,
        report.duration,
        report.errors,
        index.memory_size() / 1024
    );

    let mut lines: Vec<String> = std::env::args().skip(1).collect();
    if lines.is_empty() {
        lines = ["readme", "facture", "ext:pdf", "rep*rt", "fctr"]
            .map(String::from)
            .to_vec();
    }
    let exists = |path: &Path| path.is_dir();
    let context = ParseContext {
        home: &home,
        current_folder: &home,
        folder_exists: &exists,
    };
    for line in lines {
        let query = parse(&line, &context);
        let started = Instant::now();
        let results = index.search(&query, SearchMode::All, 5);
        let elapsed = started.elapsed();
        println!(
            "\n{line:?}: {} matches in {elapsed:.2?}{}",
            results.total,
            if results.is_fuzzy { " (fuzzy)" } else { "" }
        );
        for hit in &results.hits {
            println!("  {}", index.path_of(hit.id).display());
        }
    }
}
