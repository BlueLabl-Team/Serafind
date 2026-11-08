//! Search speed on a synthetic index of one million entries (goal: under 10 ms per query).
//!
//! Run with `cargo bench -p serafind-index`.

use std::path::Path;
use std::sync::LazyLock;

use serafind_core::{ParseContext, Query, SearchMode, parse};
use serafind_index::{Index, Kind};

const ROOT: &str = "/home/me";

const WORDS: [&str; 24] = [
    "invoice", "report", "photo", "facture", "notes", "draft", "budget", "été", "projet", "src", "main",
    "lib", "test", "data", "config", "readme", "Résumé", "backup", "music", "video", "2024", "2025", "2026",
    "final",
];
const EXTENSIONS: [&str; 8] = ["pdf", "txt", "rs", "jpg", "odt", "md", "flac", "json"];

/// One million entries: 50 000 folders of 19 files, names made of two words, deterministic.
static INDEX: LazyLock<Index> = LazyLock::new(|| {
    let mut index = Index::new();
    let root = index.add_root(Path::new(ROOT));
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % 1_000_003).unwrap()
    };
    let mut folders = vec![root];
    for n in 0..50_000 {
        let parent = folders[next() % folders.len()];
        let name = format!(
            "{}-{}-{n}",
            WORDS[next() % WORDS.len()],
            WORDS[next() % WORDS.len()]
        );
        let folder = index.insert(parent, name.as_ref(), Kind::Folder);
        folders.push(folder);
        for f in 0..19 {
            let name = format!(
                "{} {}-{f}.{}",
                WORDS[next() % WORDS.len()],
                WORDS[next() % WORDS.len()],
                EXTENSIONS[next() % EXTENSIONS.len()]
            );
            index.insert(folder, name.as_ref(), Kind::File);
        }
    }
    index
});

fn query(line: &str) -> Query {
    let context = ParseContext {
        home: Path::new(ROOT),
        current_folder: Path::new(ROOT),
        folder_exists: &|_| true,
    };
    parse(line, &context)
}

fn main() {
    let index = &*INDEX;
    eprintln!(
        "index: {} entries, about {} MiB",
        index.len(),
        index.memory_size() / (1024 * 1024)
    );
    divan::main();
}

#[divan::bench(args = ["invoice", "facture 2026", "ete", "rep*rt", "ext:pdf budget", "zzz-nothing", "fctr"])]
fn search(bencher: divan::Bencher, line: &str) {
    let index = &*INDEX;
    let query = query(line);
    bencher.bench(|| index.search(divan::black_box(&query), SearchMode::All, 50));
}

#[divan::bench]
fn search_files_mode_single_letter(bencher: divan::Bencher) {
    let index = &*INDEX;
    let query = query("e");
    bencher.bench(|| index.search(divan::black_box(&query), SearchMode::Files, 50));
}
