//! Benchmark fixtures for the runa benches.
//!
//! Wraps internal types so the benches can use them. Not part of the runa API
//! and stripped from the rn binary.
//!
//! Each fixture has a new function for the setup, an input function for a fresh input
//! and a run function which is the measured part. Only fixtures whose run consumes
//! or changes its input have an input function.
//!
//! Nothing in here writes to disk.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use dashmap::DashMap;

use crate::app::AppState;
use crate::config::Config;
use crate::core::cache::DirCache;
use crate::core::listing::{DirListOptions, SortConfig, SortMode, SortOrder};
use crate::core::metadata::CachedMetaKey;
use crate::core::workers::Workers;
use crate::core::{FileEntry, Listing, fm, preview};
use crate::runtime::POLL_INTERVAL;
use crate::utils::text::StrBuffer;

/// Directory the generated entries pretend to live in. Never touched on disk.
const SYNTHETIC_DIR: &str = "/bench";

/// Date format with the year.
/// Keeps the formatted dates the same, no matter the current date.
const DATE_FORMAT: &str = "%Y-%m-%d %H:%M";

/// Holds owned entries for the benches.
pub struct Entries(Vec<FileEntry>);

/// Raw names and flags used to create entries.
pub type Names = Vec<(OsString, u8)>;

/// Random number generator with a fixed seed.
/// Creates the same input on every run.
struct Lcg(u64);

impl Lcg {
    const fn new() -> Self {
        Self(0x2545_F491_4F6C_DD1D)
    }

    const fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }
}

fn synthetic_names(count: usize) -> Names {
    const EXTS: [&str; 8] = ["rs", "txt", "md", "toml", "png", "log", "tar.gz", ""];

    let mut rng = Lcg::new();
    (0..count)
        .map(|i| {
            let r = rng.next();
            let ext = EXTS[r % EXTS.len()];
            let name = if ext.is_empty() {
                format!("entry_{}_{}", r % 100_000, i)
            } else {
                format!("entry_{}_{}.{}", r % 100_000, i, ext)
            };
            let flags = if r.is_multiple_of(5) {
                FileEntry::IS_DIR
            } else {
                0
            };
            (OsString::from(name), flags)
        })
        .collect()
}

fn build_entries(names: Names) -> Vec<FileEntry> {
    names
        .into_iter()
        .map(|(name, flags)| FileEntry::new(name, flags, None))
        .collect()
}

/// Creates metadata for the generated entries, keyed like sort_entries looks it up.
/// Metadata sorts then never touch the disk.
fn synthetic_metadata(entries: &[FileEntry]) -> DashMap<PathBuf, CachedMetaKey> {
    const START: u64 = 1_420_070_400; // 2015-01-01
    const SPAN: u64 = 6 * 365 * 24 * 60 * 60;

    let mut rng = Lcg::new();
    let mut time = || Some(UNIX_EPOCH + Duration::from_secs(START + rng.next() as u64 % SPAN));
    let cache = DashMap::with_capacity(entries.len());
    for entry in entries {
        let key = CachedMetaKey {
            size: (!entry.is_dir()).then(|| entry.name_str().len() as u64 * 4_099),
            modified: time(),
            created: time(),
            accessed: time(),
        };
        cache.insert(Path::new(SYNTHETIC_DIR).join(entry.name()), key);
    }
    cache
}

const fn list_options() -> DirListOptions {
    DirListOptions {
        dirs_first: true,
        show_hidden: true,
        show_symlink: true,
        show_system: false,
        case_insensitive: true,
    }
}

fn listing_for(mode: SortMode) -> Listing {
    Listing::new(
        list_options(),
        SortConfig::from((mode, SortOrder::Ascending)),
        Arc::new(HashSet::new()),
    )
}

/// Sort modes measured by the benches.
/// Created and accessed are left out, since they run the same code as modified.
#[derive(Debug, Clone, Copy)]
pub enum SortBy {
    Name,
    Natural,
    Extension,
    Size,
    Modified,
}

impl SortBy {
    const fn mode(self) -> SortMode {
        match self {
            SortBy::Name => SortMode::Name,
            SortBy::Natural => SortMode::Natural,
            SortBy::Extension => SortMode::Extension,
            SortBy::Size => SortMode::Size,
            SortBy::Modified => SortMode::Modified,
        }
    }
}

/// Creates file entries, the cost paid for every entry of a directory.
pub struct EntryFixture {
    names: Names,
}

impl EntryFixture {
    pub fn new(count: usize) -> Self {
        Self {
            names: synthetic_names(count),
        }
    }

    pub fn input(&self) -> Names {
        self.names.clone()
    }

    pub fn run(&self, names: Names) -> Entries {
        Entries(build_entries(names))
    }
}

/// Sorts generated entries with the listing.
pub struct SortFixture {
    listing: Listing,
    entries: Vec<FileEntry>,
    metadata: DashMap<PathBuf, CachedMetaKey>,
}

impl SortFixture {
    pub fn new(by: SortBy, count: usize) -> Self {
        let entries = build_entries(synthetic_names(count));
        Self {
            listing: listing_for(by.mode()),
            metadata: synthetic_metadata(&entries),
            entries,
        }
    }

    pub fn input(&self) -> Entries {
        Entries(self.entries.clone())
    }

    pub fn run(&self, input: &mut Entries) -> usize {
        let column = self.listing.sort_entries(
            Path::new(SYNTHETIC_DIR),
            &mut input.0,
            DATE_FORMAT,
            &self.metadata,
        );
        input.0.len() + column.map_or(0, |c| c.len())
    }
}

/// Filters generated entries, which runs before every sort.
pub struct FilterFixture {
    listing: Listing,
    entries: Vec<FileEntry>,
}

impl FilterFixture {
    pub fn new(count: usize) -> Self {
        Self {
            listing: listing_for(SortMode::Natural),
            entries: build_entries(synthetic_names(count)),
        }
    }

    pub fn input(&self) -> Entries {
        Entries(self.entries.clone())
    }

    pub fn run(&self, input: &mut Entries) -> usize {
        self.listing.filter_entries(&mut input.0);
        input.0.len()
    }
}

/// Looks up, inserts and evicts directories in the directory cache.
pub struct CacheFixture {
    cache: DirCache,
    dirs: Vec<PathBuf>,
    entries: Arc<[FileEntry]>,
    sort_column: Option<Arc<StrBuffer>>,
    list: DirListOptions,
    sort: SortConfig,
    next_id: u64,
}

impl CacheFixture {
    /// Fills the cache with directories which share one listing.
    pub fn new(dirs: usize, entries_per_dir: usize) -> Self {
        let entries: Arc<[FileEntry]> = Arc::from(build_entries(synthetic_names(entries_per_dir)));
        let sort_column = Some(Arc::new(StrBuffer::from_iter(
            entries.iter().map(|e| e.name_str()),
        )));

        let mut fixture = Self {
            cache: DirCache::new(),
            dirs: (0..dirs)
                .map(|i| Path::new(SYNTHETIC_DIR).join(format!("dir_{i}")))
                .collect(),
            entries,
            sort_column,
            list: list_options(),
            sort: SortConfig::default(),
            next_id: 0,
        };
        for i in 0..fixture.dirs.len() {
            fixture.insert_at(i);
        }
        fixture
    }

    fn insert_at(&mut self, index: usize) {
        self.next_id += 1;
        self.cache.insert_if_newer(
            &self.dirs[index],
            self.sort,
            &self.list,
            Arc::clone(&self.entries),
            self.sort_column.clone(),
            Instant::now(),
        );
    }

    pub fn hit(&self) -> bool {
        let last = self.dirs.len().saturating_sub(1);
        self.cache
            .get(&self.dirs[last], self.sort, &self.list)
            .is_some()
    }

    pub fn miss(&self) -> bool {
        self.cache
            .get(Path::new("/bench/absent"), self.sort, &self.list)
            .is_some()
    }

    /// Inserts the directories again one after another.
    /// Includes the eviction once the cache is full at 30 directories.
    pub fn insert(&mut self) {
        let index = (self.next_id as usize) % self.dirs.len();
        self.insert_at(index);
    }

    /// Invalidates the last directory and measures the scan over all keys.
    /// The key itself is gone after the first run.
    pub fn invalidate(&self) {
        let last = self.dirs.len().saturating_sub(1);
        self.cache.invalidate_path(&self.dirs[last]);
    }
}

/// Loads a directory like the io worker does, with browsing, filtering and sorting.
/// Metadata sorts read the metadata of every entry, like the worker.
pub struct LoadFixture {
    dir: PathBuf,
    listing: Listing,
}

impl LoadFixture {
    pub fn new(dir: &Path, by: SortBy) -> Self {
        Self {
            dir: dir.to_path_buf(),
            listing: listing_for(by.mode()),
        }
    }

    /// Reads the directory without filtering or sorting.
    pub fn browse(&self) -> usize {
        fm::browse_dir(&self.dir).map_or(0, |e| e.len())
    }

    /// Runs the full directory load.
    pub fn run(&self) -> Entries {
        let Ok(mut entries) = fm::browse_dir(&self.dir) else {
            return Entries(Vec::new());
        };
        let metadata = DashMap::with_capacity(entries.len());
        self.listing.filter_entries(&mut entries);
        let _ = self
            .listing
            .sort_entries(&self.dir, &mut entries, DATE_FORMAT, &metadata);
        Entries(entries)
    }
}

/// Reads a file preview with the internal reader.
/// Previews through bat are not measured, since bat is an external process.
pub struct PreviewFixture {
    path: PathBuf,
    lines: usize,
    width: usize,
    scroll: usize,
}

impl PreviewFixture {
    pub fn new(path: &Path, lines: usize, width: usize, scroll: usize) -> Self {
        Self {
            path: path.to_path_buf(),
            lines,
            width,
            scroll,
        }
    }

    pub fn run(&self) -> Vec<String> {
        preview::safe_read_preview(&self.path, self.lines, self.width, self.scroll)
    }
}

/// Runs the app state with real workers to check the idle behaviour.
pub struct IdleFixture {
    app: AppState,
    workers: Workers,
}

impl IdleFixture {
    /// Opens the directory and waits for the first loads and previews to finish.
    pub fn new(dir: &Path) -> io::Result<Self> {
        let workers = Workers::spawn();
        let mut app = AppState::from_dir(Arc::new(Config::default()), dir)?;
        app.initialize(&workers, None);
        let mut fixture = Self { app, workers };
        fixture.run(Duration::from_millis(500));
        Ok(fixture)
    }

    /// Runs the event loop without input and returns the number of redraws.
    /// Should always be zero.
    pub fn run(&mut self, duration: Duration) -> usize {
        let end = Instant::now() + duration;
        let mut redraws = 0;
        while Instant::now() < end {
            let mut changed = false;
            while let Ok(response) = self.workers.response_rx().try_recv() {
                self.app.handle_worker_response(response, &self.workers);
                changed = true;
            }
            changed |= self.app.process_timers(&self.workers);
            redraws += usize::from(changed);
            std::thread::sleep(POLL_INTERVAL);
        }
        redraws
    }
}

/// Returns the size of a file entry in bytes and its heap allocations,
/// without and with a symlink.
pub const fn entry_layout() -> (usize, usize, usize) {
    (std::mem::size_of::<FileEntry>(), 1, 2)
}
