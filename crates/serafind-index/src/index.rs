//! The index itself: a compact table of entries, every name stored once in a shared buffer.
//!
//! Each entry keeps its name, its folded name (accents removed, lower case: compared without
//! folding again), its parent and its kind; full paths are rebuilt by walking up the parents, so
//! renaming or moving a folder changes a single entry. Removed entries are only marked, and the
//! table is compacted once enough of them pile up.

use std::ffi::{OsStr, OsString};
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use hashbrown::{DefaultHashBuilder, HashMap, HashTable};
use serafind_core::text::{self, CaseMatching};
use serafind_core::{Candidate, EntryKind, Matcher, Query, Ranked, Score, SearchMode};

/// Identifies an entry until the next compaction.
pub type EntryId = u32;

const NO_PARENT: EntryId = EntryId::MAX;

/// What an indexed entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A folder.
    Folder,
    /// A file (symbolic links are indexed as files and never followed).
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// An indexed root folder: not a search result itself.
    Root,
    Live(Kind),
    Removed,
}

/// A slice of the names buffer.
#[derive(Debug, Clone, Copy)]
struct Span {
    start: u32,
    len: u16,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    name: Span,
    folded: Span,
    parent: EntryId,
    depth: u16,
    state: State,
}

/// The file index: every folder and file under the roots, minus the exclusions.
#[derive(Default)]
pub struct Index {
    entries: Vec<Entry>,
    names: String,
    /// Finds an entry from its parent and name.
    lookup: HashTable<EntryId>,
    hasher: DefaultHashBuilder,
    /// Names that are not valid Unicode (possible on Linux), kept exactly to rebuild paths.
    raw_names: HashMap<EntryId, OsString>,
    removed: usize,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The entry (valid until the index changes).
    pub id: EntryId,
    /// How well it matched.
    pub score: Score,
}

/// The result of [`Index::search`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchResults {
    /// The best entries, best first.
    pub hits: Vec<Hit>,
    /// How many entries matched in all.
    pub total: usize,
    /// True when the hits come from the fuzzy fallback.
    pub is_fuzzy: bool,
    /// True when `in:` names a folder that is not in the index (nothing was searched).
    pub outside_index: bool,
}

impl Index {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live entries, roots included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len() - self.removed
    }

    /// True when nothing is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes used by the index tables, roughly (for diagnostics and benchmarks).
    #[must_use]
    pub fn memory_size(&self) -> usize {
        self.entries.capacity() * size_of::<Entry>()
            + self.names.capacity()
            + self.lookup.capacity() * (size_of::<EntryId>() + 1)
    }

    /// Gives back the room reserved for growth (after a full walk).
    pub fn shrink_to_fit(&mut self) {
        self.entries.shrink_to_fit();
        self.names.shrink_to_fit();
        let (entries, names, raw_names, hasher) = (&self.entries, &self.names, &self.raw_names, &self.hasher);
        self.lookup.shrink_to_fit(|&id| {
            let entry = entries[id as usize];
            hasher.hash_one((entry.parent, os_name_in(names, raw_names, &entry, id)))
        });
    }

    /// Adds a root folder (its full path is its name). Returns the existing one when already there.
    pub fn add_root(&mut self, path: &Path) -> EntryId {
        if let Some(id) = self.find(path) {
            return id;
        }
        self.push(NO_PARENT, path.as_os_str(), 0, State::Root)
    }

    /// Adds an entry under `parent`, or returns the existing one (its kind updated).
    ///
    /// # Panics
    /// When `parent` is not a live folder or root.
    pub fn insert(&mut self, parent: EntryId, name: &OsStr, kind: Kind) -> EntryId {
        let parent_entry = self.entries[parent as usize];
        assert!(
            matches!(parent_entry.state, State::Root | State::Live(Kind::Folder)),
            "the parent of an entry must be a folder"
        );
        if let Some(id) = self.child(parent, name) {
            self.entries[id as usize].state = State::Live(kind);
            return id;
        }
        self.push(
            parent,
            name,
            parent_entry.depth.saturating_add(1),
            State::Live(kind),
        )
    }

    /// Adds the entry at `path` (and the missing folders above it, under an existing root).
    /// Returns `None` when the path is under no root.
    pub fn insert_path(&mut self, path: &Path, kind: Kind) -> Option<EntryId> {
        let (root, rest) = self.root_of(path)?;
        let mut current = root;
        let mut components = rest.components().peekable();
        while let Some(component) = components.next() {
            let is_last = components.peek().is_none();
            let entry_kind = if is_last { kind } else { Kind::Folder };
            current = self.insert(current, component.as_os_str(), entry_kind);
        }
        Some(current)
    }

    /// The entry at `path`, if indexed.
    #[must_use]
    pub fn find(&self, path: &Path) -> Option<EntryId> {
        let (root, rest) = self.root_of(path)?;
        rest.components().try_fold(root, |parent, component| {
            self.child(parent, component.as_os_str())
        })
    }

    /// Removes the entry at `path` and everything under it. Returns how many entries were removed.
    pub fn remove_path(&mut self, path: &Path) -> usize {
        match self.find(path) {
            Some(id) if self.entries[id as usize].state != State::Root => self.remove(id),
            _ => 0,
        }
    }

    /// Moves or renames the entry at `from` to `to`; what is under a folder follows. Returns the
    /// entry, or `None` when `from` is not indexed or `to` is under no root (then `from` is removed).
    pub fn rename_path(&mut self, from: &Path, to: &Path) -> Option<EntryId> {
        let id = self.find(from)?;
        let state = self.entries[id as usize].state;
        let State::Live(kind) = state else {
            return None;
        };
        let (Some(new_parent_path), Some(new_name)) = (to.parent(), to.file_name()) else {
            self.remove(id);
            return None;
        };
        let Some(new_parent) = self.insert_path_folder(new_parent_path) else {
            self.remove(id);
            return None;
        };
        if self.is_under(new_parent, id) {
            // A folder moved into itself: impossible on a real disk; ignore.
            return Some(id);
        }
        match self.child(new_parent, new_name) {
            Some(existing) if existing == id => return Some(id),
            // Replaced an existing entry.
            Some(existing) => {
                self.remove(existing);
            }
            None => {}
        }
        self.unlink(id);
        let name = self.store_name(new_name);
        let depth = self.entries[new_parent as usize].depth.saturating_add(1);
        let entry = &mut self.entries[id as usize];
        entry.parent = new_parent;
        entry.name = name.0;
        entry.folded = name.1;
        entry.state = State::Live(kind);
        let delta_from = entry.depth;
        entry.depth = depth;
        if kind == Kind::Folder && delta_from != depth {
            self.fix_depths(id);
        }
        self.link(id);
        if let Some(raw) = name.2 {
            self.raw_names.insert(id, raw);
        } else {
            self.raw_names.remove(&id);
        }
        Some(id)
    }

    /// The full path of an entry.
    #[must_use]
    pub fn path_of(&self, id: EntryId) -> PathBuf {
        let mut chain = Vec::new();
        let mut current = id;
        while current != NO_PARENT {
            chain.push(current);
            current = self.entries[current as usize].parent;
        }
        let mut path = PathBuf::new();
        for &id in chain.iter().rev() {
            path.push(self.os_name(id));
        }
        path
    }

    /// The name of an entry (lossy when it is not valid Unicode).
    #[must_use]
    pub fn name(&self, id: EntryId) -> &str {
        self.text(self.entries[id as usize].name)
    }

    /// Every live entry, roots excluded.
    pub fn ids(&self) -> impl Iterator<Item = EntryId> + '_ {
        (0..)
            .zip(&self.entries)
            .filter(|(_, entry)| matches!(entry.state, State::Live(_)))
            .map(|(id, _)| id)
    }

    /// The kind of a live entry; `None` for roots and removed entries.
    #[must_use]
    pub fn kind(&self, id: EntryId) -> Option<Kind> {
        match self.entries[id as usize].state {
            State::Live(kind) => Some(kind),
            State::Root | State::Removed => None,
        }
    }

    /// Searches the index. `tab` is the mode chosen in the bar; a mode filter of the query wins.
    #[must_use]
    pub fn search(&self, query: &Query, tab: SearchMode, limit: usize) -> SearchResults {
        let mode = query.effective_mode(tab);
        if mode == SearchMode::Apps {
            return SearchResults::default();
        }
        let start = match &query.start_folder {
            Some(folder) => match self.find(folder) {
                Some(id) => Some(id),
                None => {
                    return SearchResults {
                        outside_index: true,
                        ..SearchResults::default()
                    };
                }
            },
            None => None,
        };
        let Ranked {
            entries,
            total,
            is_fuzzy,
        } = self.scan_in_parallel(query, mode, start, limit);
        SearchResults {
            hits: entries
                .into_iter()
                .map(|(score, candidate)| Hit {
                    id: candidate.id,
                    score,
                })
                .collect(),
            total,
            is_fuzzy,
            outside_index: false,
        }
    }

    /// Scans the table in slices, one thread per slice, and puts the best matches together.
    fn scan_in_parallel(
        &self,
        query: &Query,
        mode: SearchMode,
        start: Option<EntryId>,
        limit: usize,
    ) -> Ranked<'_> {
        const MIN_SLICE: usize = 32 * 1024;
        let threads = std::thread::available_parallelism().map_or(1, |count| count.get().min(8));
        let slices = (self.entries.len() / MIN_SLICE).clamp(1, threads);
        let slice_length = self.entries.len().div_ceil(slices).max(1);
        let scan = |first: usize| {
            let end = (first + slice_length).min(self.entries.len());
            let first_id = EntryId::try_from(first).expect("ids fit in u32");
            let candidates = self.entries[first..end]
                .iter()
                .zip(first_id..)
                .filter_map(|(entry, id)| self.candidate(entry, id, start));
            Matcher::new(query, mode).scan(candidates, limit)
        };
        if slices == 1 {
            return scan(0).finish();
        }
        std::thread::scope(|scope| {
            let workers: Vec<_> = (1..slices)
                .map(|slice| scope.spawn(move || scan(slice * slice_length)))
                .collect();
            let mut partial = scan(0);
            for worker in workers {
                partial.merge(worker.join().expect("a search thread panicked"));
            }
            partial.finish()
        })
    }

    fn candidate(&self, entry: &Entry, id: EntryId, start: Option<EntryId>) -> Option<Candidate<'_>> {
        let State::Live(kind) = entry.state else {
            return None;
        };
        if start.is_some_and(|start| !self.is_strictly_under(id, start)) {
            return None;
        }
        Some(Candidate {
            name: self.text(entry.name),
            folded_name: Some(self.text(entry.folded)),
            depth: entry.depth,
            kind: match kind {
                Kind::Folder => EntryKind::Folder,
                Kind::File => EntryKind::File,
            },
            id,
        })
    }

    /// Rebuilds the tables without the removed entries when they take too much room. Entry ids
    /// change: earlier results must not be used afterwards. Returns whether it compacted.
    pub fn compact_if_needed(&mut self) -> bool {
        if self.removed < 1024 || self.removed * 4 < self.entries.len() {
            return false;
        }
        let old = std::mem::take(self);
        let mut new_ids = vec![NO_PARENT; old.entries.len()];
        // Parents always come before their children in the table, except after a move: insert in
        // depth order to be safe.
        let mut order: Vec<EntryId> = (0..)
            .zip(&old.entries)
            .filter(|(_, entry)| entry.state != State::Removed)
            .map(|(id, _)| id)
            .collect();
        order.sort_by_key(|&id| old.entries[id as usize].depth);
        for id in order {
            let entry = old.entries[id as usize];
            let name = old.os_name(id);
            new_ids[id as usize] = match entry.state {
                State::Root => self.push(NO_PARENT, name, 0, State::Root),
                State::Live(_) => self.push(new_ids[entry.parent as usize], name, entry.depth, entry.state),
                State::Removed => unreachable!("removed entries are filtered out"),
            };
        }
        true
    }

    // --- Internals ---

    fn push(&mut self, parent: EntryId, name: &OsStr, depth: u16, state: State) -> EntryId {
        let id = EntryId::try_from(self.entries.len()).expect("fewer than 4 billion entries");
        let (name, folded, raw) = self.store_name(name);
        self.entries.push(Entry {
            name,
            folded,
            parent,
            depth,
            state,
        });
        if let Some(raw) = raw {
            self.raw_names.insert(id, raw);
        }
        self.link(id);
        id
    }

    /// Stores a name and its folded form; also returns the exact name when it is not Unicode.
    fn store_name(&mut self, name: &OsStr) -> (Span, Span, Option<OsString>) {
        let lossy = name.to_string_lossy();
        let raw = name.to_str().is_none().then(|| name.to_owned());
        let name_span = append(&mut self.names, &lossy);
        let folded = text::fold(&lossy, CaseMatching::Ignore);
        let folded_span = if folded == lossy {
            name_span
        } else {
            append(&mut self.names, &folded)
        };
        (name_span, folded_span, raw)
    }

    fn text(&self, span: Span) -> &str {
        let start = span.start as usize;
        &self.names[start..start + usize::from(span.len)]
    }

    fn os_name(&self, id: EntryId) -> &OsStr {
        self.raw_names
            .get(&id)
            .map_or_else(|| OsStr::new(self.name(id)), OsString::as_os_str)
    }

    fn hash_of(&self, parent: EntryId, name: &OsStr) -> u64 {
        self.hasher.hash_one((parent, name))
    }

    fn link(&mut self, id: EntryId) {
        let entry = self.entries[id as usize];
        let hash = self.hash_of(entry.parent, self.os_name(id));
        let (entries, names, raw_names, hasher) = (&self.entries, &self.names, &self.raw_names, &self.hasher);
        self.lookup.insert_unique(hash, id, |&other| {
            let other_entry = entries[other as usize];
            hasher.hash_one((
                other_entry.parent,
                os_name_in(names, raw_names, &other_entry, other),
            ))
        });
    }

    fn unlink(&mut self, id: EntryId) {
        let entry = self.entries[id as usize];
        let hash = self.hash_of(entry.parent, self.os_name(id));
        if let Ok(found) = self.lookup.find_entry(hash, |&other| other == id) {
            found.remove();
        }
    }

    fn child(&self, parent: EntryId, name: &OsStr) -> Option<EntryId> {
        let hash = self.hash_of(parent, name);
        self.lookup
            .find(hash, |&id| {
                let entry = self.entries[id as usize];
                entry.parent == parent && entry.state != State::Removed && self.os_name(id) == name
            })
            .copied()
    }

    /// The root containing `path`, and the rest of the path under it (deepest root first).
    fn root_of<'p>(&self, path: &'p Path) -> Option<(EntryId, &'p Path)> {
        self.entries
            .iter()
            .zip(0..)
            .filter(|(entry, _)| entry.state == State::Root)
            .filter_map(|(_, id)| {
                let root = PathBuf::from(self.os_name(id));
                path.strip_prefix(&root)
                    .ok()
                    .map(|rest| (id, rest, root.components().count()))
            })
            .max_by_key(|&(_, _, length)| length)
            .map(|(id, rest, _)| (id, rest))
    }

    fn insert_path_folder(&mut self, path: &Path) -> Option<EntryId> {
        if let Some(id) = self.find(path) {
            return Some(id);
        }
        self.insert_path(path, Kind::Folder)
    }

    fn remove(&mut self, id: EntryId) -> usize {
        let mut removed = 0;
        let doomed: Vec<EntryId> = (0..)
            .zip(&self.entries)
            .filter(|&(other, entry)| entry.state != State::Removed && self.is_under(other, id))
            .map(|(other, _)| other)
            .collect();
        for other in doomed {
            self.unlink(other);
            self.entries[other as usize].state = State::Removed;
            self.raw_names.remove(&other);
            removed += 1;
        }
        self.removed += removed;
        removed
    }

    /// True when `id` is `ancestor` or under it.
    fn is_under(&self, id: EntryId, ancestor: EntryId) -> bool {
        id == ancestor || self.is_strictly_under(id, ancestor)
    }

    fn is_strictly_under(&self, id: EntryId, ancestor: EntryId) -> bool {
        let ancestor_depth = self.entries[ancestor as usize].depth;
        let mut current = self.entries[id as usize].parent;
        while current != NO_PARENT {
            if current == ancestor {
                return true;
            }
            let entry = self.entries[current as usize];
            if entry.depth < ancestor_depth {
                return false;
            }
            current = entry.parent;
        }
        false
    }

    fn fix_depths(&mut self, folder: EntryId) {
        let below: Vec<EntryId> = (0..)
            .zip(&self.entries)
            .filter(|&(other, entry)| entry.state != State::Removed && self.is_strictly_under(other, folder))
            .map(|(other, _)| other)
            .collect();
        // Parents may come after children in the table: recompute from the chain each time.
        for id in below {
            let mut depth = 0u16;
            let mut current = self.entries[id as usize].parent;
            while current != NO_PARENT {
                depth = depth.saturating_add(1);
                current = self.entries[current as usize].parent;
            }
            self.entries[id as usize].depth = depth;
        }
    }
}

fn append(buffer: &mut String, text: &str) -> Span {
    let start = u32::try_from(buffer.len()).expect("names fit in 4 GiB");
    // Names longer than 64 KiB do not exist on real file systems; cut at a char boundary to be safe.
    let mut end = text.len().min(usize::from(u16::MAX));
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    buffer.push_str(&text[..end]);
    Span {
        start,
        len: u16::try_from(end).expect("cut to u16 above"),
    }
}

fn os_name_in<'a>(
    names: &'a str,
    raw_names: &'a HashMap<EntryId, OsString>,
    entry: &Entry,
    id: EntryId,
) -> &'a OsStr {
    raw_names.get(&id).map_or_else(
        || {
            let start = entry.name.start as usize;
            OsStr::new(&names[start..start + usize::from(entry.name.len)])
        },
        OsString::as_os_str,
    )
}

#[cfg(test)]
mod tests;
