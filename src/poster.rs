//! Production posters (ADR-0011, ADR-0012). A poster goes TMDB → disk cache
//! → bounded RAM cache of decoded pixels → rows. The UI thread only looks up
//! RAM; disk reads, downloads and decoding run on the network workers, and a
//! finished load is announced by key so every view re-reads its own rows.
//!
//! The benchmark fixture keeps its own synchronous pipeline
//! (`fixture/poster.rs`); nothing here touches it.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use slint::{Image, Rgb8Pixel, SharedPixelBuffer};

use crate::diagnostics::Log;
use crate::network::Network;
use crate::tmdb::{TmdbClient, TmdbError};

/// The one TMDB poster size: sharp in 40×60 rows at any scale and in the
/// 160×240 detail pane at 100 %.
pub const SIZE: &str = "w185";
/// Decoded bytes the RAM cache may hold, as in R3: about 81 `w185` posters.
pub const RAM_BUDGET: usize = 12 * 1024 * 1024;
/// How soon a poster that failed for a transient reason may be tried again.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// Larger images are not posters; refuse them before decoding.
const MAX_SIDE: u32 = 4096;
/// Temp files older than this belong to no running download.
const STALE_TEMP: Duration = Duration::from_secs(3600);
const PREFIX: &str = "tmdb-w185-";

/// A poster file at TMDB in our size. Built only from a validated path, so
/// its name is safe as a file name: no separators, no traversal, and one name
/// per path (no collisions between different posters).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PosterKey(String);

impl PosterKey {
    /// `/` + 1–64 of `[A-Za-z0-9_-]` + `.jpg`, `.jpeg` or `.png`, as TMDB's
    /// poster paths are. Anything else is "no poster".
    pub fn tmdb(path: &str) -> Option<Self> {
        let (stem, ext) = path.strip_prefix('/')?.rsplit_once('.')?;
        let stem_ok = (1..=64).contains(&stem.len())
            && stem
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        (stem_ok && matches!(ext, "jpg" | "jpeg" | "png"))
            .then(|| Self(format!("{PREFIX}{stem}.{ext}")))
    }

    /// The cache file name, also the key's text form.
    pub fn name(&self) -> &str {
        &self.0
    }

    /// The TMDB file path, for the download URL.
    fn tmdb_path(&self) -> String {
        format!("/{}", &self.0[PREFIX.len()..])
    }
}

/// The poster files, one per key, under `<cache dir>/posters`.
pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn read(&self, key: &PosterKey) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.dir.join(key.name())) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Writes to a temp file, syncs it, then renames it over the final name,
    /// so a partial file never carries a final name.
    fn write(&self, key: &PosterKey, bytes: &[u8]) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let temp = self.dir.join(format!(
            ".tmp-{}-{nanos}-{}",
            std::process::id(),
            key.name()
        ));
        let written = File::create_new(&temp).and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
        let result = written.and_then(|()| fs::rename(&temp, self.dir.join(key.name())));
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    fn remove(&self, key: &PosterKey) {
        let _ = fs::remove_file(self.dir.join(key.name()));
    }

    /// Removes temp files left by downloads that died more than an hour ago.
    /// Returns how many it removed.
    pub fn clean_temp(&self) -> usize {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return 0;
        };
        entries
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".tmp-"))
            .filter(|entry| {
                let age = entry.metadata().and_then(|m| m.modified()).ok();
                age.and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > STALE_TEMP)
            })
            .filter(|entry| fs::remove_file(entry.path()).is_ok())
            .count()
    }
}

/// Decodes and checks a poster. Refuses anything that is not a readable JPEG
/// or PNG of plausible size, before allocating for it.
pub fn decode(bytes: &[u8]) -> Result<SharedPixelBuffer<Rgb8Pixel>, String> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    let mut reader = image::ImageReader::new(io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|err| err.to_string())?;
    reader.limits(limits);
    let image = reader.decode().map_err(|err| err.to_string())?.to_rgb8();
    Ok(SharedPixelBuffer::clone_from_slice(
        image.as_raw(),
        image.width(),
        image.height(),
    ))
}

fn decoded_bytes(pixels: &SharedPixelBuffer<Rgb8Pixel>) -> usize {
    pixels.width() as usize * pixels.height() as usize * 3
}

/// Least-recently-used decoded posters, never above `budget` bytes.
struct Lru {
    budget: usize,
    used: usize,
    // ponytail: linear scan; fine for the ~80 entries the budget allows.
    /// (key, pixels, bytes), least recently used first.
    entries: Vec<(PosterKey, SharedPixelBuffer<Rgb8Pixel>, usize)>,
}

impl Lru {
    fn get(&mut self, key: &PosterKey) -> Option<SharedPixelBuffer<Rgb8Pixel>> {
        let index = self.entries.iter().position(|entry| entry.0 == *key)?;
        let entry = self.entries.remove(index);
        let pixels = entry.1.clone();
        self.entries.push(entry);
        Some(pixels)
    }

    /// Returns how many entries were evicted to make room.
    fn insert(&mut self, key: PosterKey, pixels: SharedPixelBuffer<Rgb8Pixel>) -> u64 {
        let bytes = decoded_bytes(&pixels);
        if bytes > self.budget || self.entries.iter().any(|entry| entry.0 == key) {
            return 0;
        }
        let mut evicted = 0;
        while self.used + bytes > self.budget {
            let (_, _, freed) = self.entries.remove(0);
            self.used -= freed;
            evicted += 1;
        }
        self.used += bytes;
        self.entries.push((key, pixels, bytes));
        evicted
    }
}

/// Counters since startup, logged once at exit.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stats {
    pub ram_hits: u64,
    pub disk_loads: u64,
    pub downloads: u64,
    pub failures: u64,
    pub evictions: u64,
}

impl fmt::Display for Stats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "posters: {} RAM hits, {} disk loads, {} downloads, {} failures, {} evictions",
            self.ram_hits, self.disk_loads, self.downloads, self.failures, self.evictions
        )
    }
}

enum Outcome {
    Loaded {
        pixels: SharedPixelBuffer<Rgb8Pixel>,
        downloaded: bool,
        /// The image is fine but could not be kept on disk.
        not_saved: Option<io::Error>,
    },
    Failed {
        permanent: bool,
        why: String,
    },
}

struct State {
    ram: Lru,
    in_flight: HashSet<PosterKey>,
    /// Failed keys: `None` never again this session, `Some(t)` not before `t`.
    failed: HashMap<PosterKey, Option<Instant>>,
    base_url: String,
    stats: Stats,
}

/// Called on the UI thread when a poster is ready, with its key.
pub type Ready = Box<dyn Fn(&PosterKey) + Send + Sync>;

/// The poster service shared by the Library and Discover.
pub struct Posters {
    disk: Option<DiskCache>,
    client: TmdbClient,
    network: Network,
    log: Arc<Log>,
    ready: Ready,
    state: Mutex<State>,
}

impl Posters {
    /// `dir` is the poster folder, or `None` when there is no cache folder
    /// (posters then live in RAM only).
    pub fn new(
        dir: Option<PathBuf>,
        client: TmdbClient,
        network: Network,
        log: Arc<Log>,
        ready: Ready,
    ) -> Self {
        let disk = dir.map(DiskCache::new);
        if let Some(removed) = disk.as_ref().map(DiskCache::clean_temp).filter(|n| *n > 0) {
            log.info(format_args!(
                "Removed {removed} stale temporary poster files"
            ));
        }
        Self {
            disk,
            client,
            network,
            log,
            ready,
            state: Mutex::new(State {
                ram: Lru {
                    budget: RAM_BUDGET,
                    used: 0,
                    entries: Vec::new(),
                },
                in_flight: HashSet::new(),
                failed: HashMap::new(),
                base_url: crate::tmdb::IMAGE_BASE.to_owned(),
                stats: Stats::default(),
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Uses the image base URL from TMDB's `/3/configuration`.
    pub fn set_base_url(&self, url: String) {
        self.state().base_url = url;
    }

    pub fn stats(&self) -> Stats {
        self.state().stats
    }

    /// No load is running (tests wait for this).
    #[cfg(test)]
    pub fn idle(&self) -> bool {
        self.state().in_flight.is_empty()
    }

    /// Decoded bytes held in RAM.
    #[cfg(test)]
    pub fn ram_bytes(&self) -> usize {
        self.state().ram.used
    }

    /// The poster if it is in RAM. Otherwise starts loading it (unless it is
    /// already loading or recently failed) and returns `None`: the caller
    /// shows the placeholder and hears about the poster through `ready`.
    pub fn image(self: &Arc<Self>, key: &PosterKey) -> Option<Image> {
        let mut state = self.state();
        if let Some(pixels) = state.ram.get(key) {
            state.stats.ram_hits += 1;
            return Some(Image::from_rgb8(pixels));
        }
        let blocked = match state.failed.get(key) {
            Some(None) => true,
            Some(Some(until)) => Instant::now() < *until,
            None => false,
        };
        if blocked || !state.in_flight.insert(key.clone()) {
            return None;
        }
        let base_url = state.base_url.clone();
        drop(state);
        let (posters, key) = (self.clone(), key.clone());
        self.network.run(move || {
            let outcome = posters.load(&key, &base_url);
            let back = posters.clone();
            posters.network.post(move || back.arrived(key, outcome));
        });
        None
    }

    /// Worker side: disk first, then TMDB. A download is decoded (checked)
    /// before it is written.
    fn load(&self, key: &PosterKey, base_url: &str) -> Outcome {
        if let Some(disk) = &self.disk {
            match disk.read(key) {
                Ok(Some(bytes)) => match decode(&bytes) {
                    Ok(pixels) => {
                        return Outcome::Loaded {
                            pixels,
                            downloaded: false,
                            not_saved: None,
                        };
                    }
                    // Our own file, damaged: replace it with a fresh download.
                    Err(_) => disk.remove(key),
                },
                Ok(None) => {}
                // Unreadable cache: still show the poster from the network.
                Err(_) => {}
            }
        }
        let url = format!("{base_url}{SIZE}{}", key.tmdb_path());
        let bytes = match self.client.get_image(&url) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Outcome::Failed {
                    permanent: error == TmdbError::NotFound,
                    why: error.to_string(),
                };
            }
        };
        let pixels = match decode(&bytes) {
            Ok(pixels) => pixels,
            Err(why) => {
                return Outcome::Failed {
                    permanent: true,
                    why: format!("not a usable image: {why}"),
                };
            }
        };
        let not_saved = self
            .disk
            .as_ref()
            .and_then(|disk| disk.write(key, &bytes).err());
        Outcome::Loaded {
            pixels,
            downloaded: true,
            not_saved,
        }
    }

    /// UI side: caches the pixels (or remembers the failure), then announces
    /// the key.
    fn arrived(&self, key: PosterKey, outcome: Outcome) {
        let mut state = self.state();
        state.in_flight.remove(&key);
        match outcome {
            Outcome::Loaded {
                pixels,
                downloaded,
                not_saved,
            } => {
                if downloaded {
                    state.stats.downloads += 1;
                } else {
                    state.stats.disk_loads += 1;
                }
                state.stats.evictions += state.ram.insert(key.clone(), pixels);
                state.failed.remove(&key);
                drop(state);
                if let Some(error) = not_saved {
                    self.log.error(format_args!(
                        "Poster {} could not be saved to the cache: {error}",
                        key.name()
                    ));
                }
                (self.ready)(&key);
            }
            Outcome::Failed { permanent, why } => {
                state.stats.failures += 1;
                let retry = (!permanent).then(|| Instant::now() + RETRY_AFTER);
                // Log the first failure of each poster only.
                if state.failed.insert(key.clone(), retry).is_none() {
                    self.log
                        .info(format_args!("Poster {} not available: {why}", key.name()));
                }
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::paths::TestDir;
    use crate::tmdb::fake::{FakeServer, Reply};

    /// A real JPEG of `width`×`height` in one color.
    pub fn jpeg(width: u32, height: u32, shade: u8) -> Vec<u8> {
        let image = image::RgbImage::from_pixel(width, height, image::Rgb([shade, 90, 160]));
        let mut bytes = io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        bytes.into_inner()
    }

    fn key(path: &str) -> PosterKey {
        PosterKey::tmdb(path).unwrap()
    }

    /// A poster service that never reaches a server, for tests of other parts.
    pub fn offline() -> Arc<Posters> {
        let client = TmdbClient::for_tests("http://127.0.0.1:9", Duration::from_secs(1));
        let network = Network::with_post(1, Arc::new(|job: crate::network::Job| drop(job)));
        let log = Arc::new(Log::stderr_only());
        Arc::new(Posters::new(None, client, network, log, Box::new(|_| {})))
    }

    #[test]
    fn keys_accept_tmdb_paths_only() {
        let poster = key("/kqjL17yufvn9OVLyXYpvtyrFfak.jpg");
        assert_eq!(poster.name(), "tmdb-w185-kqjL17yufvn9OVLyXYpvtyrFfak.jpg");
        assert_eq!(poster.tmdb_path(), "/kqjL17yufvn9OVLyXYpvtyrFfak.jpg");
        assert!(PosterKey::tmdb("/a-b_C9.png").is_some());
        for bad in [
            "",
            "/",
            "/.jpg",
            "x.jpg",
            "/../../etc/passwd.jpg",
            "/..\\..\\evil.jpg",
            "/a/b.jpg",
            "/a b.jpg",
            "/a.jpg.exe",
            "/a.exe",
            "/a.JPG",
            "/C:.jpg",
            "/%2e%2e.jpg",
            "/ä.jpg",
            &format!("/{}.jpg", "x".repeat(65)),
        ] {
            assert!(PosterKey::tmdb(bad).is_none(), "{bad:?}");
        }
        // Different posters never share a file; the key ignores the title.
        assert_ne!(key("/a.jpg"), key("/a.png"));
        assert_ne!(key("/a.jpg"), key("/A.jpg"));
    }

    #[test]
    fn disk_writes_are_atomic_and_stale_temp_files_go() {
        let dir = TestDir::new("poster-disk");
        let disk = DiskCache::new(dir.0.join("posters"));
        let poster = key("/p.jpg");
        assert_eq!(disk.read(&poster).unwrap(), None);
        disk.write(&poster, b"first").unwrap();
        disk.write(&poster, b"second").unwrap();
        assert_eq!(disk.read(&poster).unwrap().as_deref(), Some(&b"second"[..]));
        let names: Vec<_> = fs::read_dir(dir.0.join("posters"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["tmdb-w185-p.jpg"], "no temp file left");

        // A fresh temp file may belong to a running download: kept.
        let fresh = dir.0.join("posters/.tmp-1-2-tmdb-w185-x.jpg");
        fs::write(&fresh, b"partial").unwrap();
        assert_eq!(disk.clean_temp(), 0);
        assert_eq!(
            disk.read(&key("/x.jpg")).unwrap(),
            None,
            "never a final name"
        );
        // Two hours old: a dead download's leftover, removed.
        let stale = dir.0.join("posters/.tmp-1-1-tmdb-w185-y.jpg");
        fs::write(&stale, b"partial").unwrap();
        let old = SystemTime::now() - Duration::from_secs(7200);
        let file = File::options().write(true).open(&stale).unwrap();
        file.set_modified(old).unwrap();
        drop(file);
        assert_eq!(disk.clean_temp(), 1);
        assert!(!stale.exists() && fresh.exists());
        assert!(
            disk.read(&poster).unwrap().is_some(),
            "cached posters untouched"
        );
    }

    #[test]
    fn decode_rejects_junk_and_huge_images() {
        let pixels = decode(&jpeg(185, 278, 10)).unwrap();
        assert_eq!((pixels.width(), pixels.height()), (185, 278));
        assert_eq!(decoded_bytes(&pixels), 185 * 278 * 3);
        assert!(decode(b"").is_err());
        assert!(decode(b"<html>not found</html>").is_err());
        let truncated = jpeg(185, 278, 10);
        assert!(decode(&truncated[..40]).is_err());
        assert!(decode(&jpeg(4097, 2, 10)).is_err(), "over the size limit");
    }

    fn lru(budget: usize) -> Lru {
        Lru {
            budget,
            used: 0,
            entries: Vec::new(),
        }
    }

    fn pixels(width: u32) -> SharedPixelBuffer<Rgb8Pixel> {
        SharedPixelBuffer::new(width, 1)
    }

    #[test]
    fn lru_evicts_least_recently_used_within_budget() {
        let mut ram = lru(27); // three 9-byte entries (3 × 1 pixels × 3 bytes)
        for name in ["/a.jpg", "/b.jpg", "/c.jpg"] {
            assert_eq!(ram.insert(key(name), pixels(3)), 0);
        }
        assert!(ram.get(&key("/a.jpg")).is_some()); // a is now the newest
        assert_eq!(ram.insert(key("/d.jpg"), pixels(3)), 1);
        assert!(ram.get(&key("/b.jpg")).is_none(), "b was the oldest");
        assert!(ram.get(&key("/a.jpg")).is_some());
        assert!(ram.used <= ram.budget);
        assert_eq!(
            ram.insert(key("/huge.jpg"), pixels(100)),
            0,
            "bigger than the budget"
        );
        assert!(ram.get(&key("/huge.jpg")).is_none());
        assert_eq!(ram.entries.len(), 3);
    }

    /// A poster service over a fake image server, with posts pumped by hand.
    pub struct Harness {
        pub posters: Arc<Posters>,
        pub ready: Arc<Mutex<Vec<String>>>,
        posted: Arc<Mutex<Vec<crate::network::Job>>>,
    }

    impl Harness {
        pub fn new(dir: Option<PathBuf>, server: &FakeServer) -> Self {
            let posted: Arc<Mutex<Vec<crate::network::Job>>> = Arc::default();
            let queue = posted.clone();
            let network =
                Network::with_post(4, Arc::new(move |job| queue.lock().unwrap().push(job)));
            let ready = Arc::new(Mutex::new(Vec::new()));
            let heard = ready.clone();
            let posters = Arc::new(Posters::new(
                dir,
                TmdbClient::for_tests(&server.base, Duration::from_secs(5)),
                network,
                Arc::new(Log::stderr_only()),
                Box::new(move |key: &PosterKey| heard.lock().unwrap().push(key.name().to_owned())),
            ));
            posters.set_base_url(format!("{}/t/p/", server.base));
            Self {
                posters,
                ready,
                posted,
            }
        }

        /// Runs posted results until `n` loads have finished (10 s at most).
        pub fn settle(&self, n: u64) {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let jobs = std::mem::take(&mut *self.posted.lock().unwrap());
                jobs.into_iter().for_each(|job| job());
                let s = self.posters.stats();
                if s.disk_loads + s.downloads + s.failures >= n {
                    return;
                }
                assert!(Instant::now() < deadline, "{s:?}");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    fn image_server() -> FakeServer {
        FakeServer::start(|seen| match seen.target.as_str() {
            "/t/p/w185/good.jpg" | "/t/p/w185/other.jpg" => {
                Reply::bytes(200, jpeg(185, 278, 40)).after(Duration::from_millis(150))
            }
            "/t/p/w185/corrupt.jpg" => Reply::bytes(200, b"\xff\xd8 not really".to_vec()),
            "/t/p/w185/cut.jpg" => Reply::bytes(200, jpeg(185, 278, 40)).truncated(),
            "/t/p/w185/down.jpg" => Reply::json(503, "{}"),
            _ => Reply::json(404, r#"{"status_code":34}"#),
        })
    }

    fn requests(server: &FakeServer, path: &str) -> usize {
        server
            .seen()
            .iter()
            .filter(|s| s.target.ends_with(path))
            .count()
    }

    #[test]
    fn ram_then_disk_then_network_with_one_download_per_poster() {
        let server = image_server();
        let dir = TestDir::new("poster-flow");
        let posters_dir = dir.0.join("posters");
        let h = Harness::new(Some(posters_dir.clone()), &server);
        let good = key("/good.jpg");

        // Three rows ask at once: one download, one announcement.
        for _ in 0..3 {
            assert!(h.posters.image(&good).is_none());
        }
        h.settle(1);
        assert_eq!(requests(&server, "/good.jpg"), 1);
        assert_eq!(*h.ready.lock().unwrap(), ["tmdb-w185-good.jpg"]);
        assert!(posters_dir.join("tmdb-w185-good.jpg").is_file());
        let image = h.posters.image(&good).expect("now in RAM");
        assert_eq!((image.size().width, image.size().height), (185, 278));
        assert_eq!(h.posters.stats().ram_hits, 1);

        // A new session: the disk, not the network.
        let again = Harness::new(Some(posters_dir), &server);
        assert!(again.posters.image(&good).is_none());
        again.settle(1);
        let stats = again.posters.stats();
        assert_eq!((stats.disk_loads, stats.downloads), (1, 0));
        assert_eq!(requests(&server, "/good.jpg"), 1, "still one download");
        assert!(again.posters.image(&good).is_some());
    }

    #[test]
    fn failures_leave_the_placeholder_and_nothing_on_disk() {
        let server = image_server();
        let dir = TestDir::new("poster-failures");
        let posters_dir = dir.0.join("posters");
        let h = Harness::new(Some(posters_dir.clone()), &server);
        let paths = ["/corrupt.jpg", "/cut.jpg", "/down.jpg", "/missing.jpg"];
        for path in paths {
            assert!(h.posters.image(&key(path)).is_none());
        }
        h.settle(4);
        let stats = h.posters.stats();
        assert_eq!((stats.failures, stats.downloads), (4, 0));
        assert!(h.ready.lock().unwrap().is_empty(), "nothing announced");
        assert!(!posters_dir.exists() || fs::read_dir(&posters_dir).unwrap().next().is_none());
        // Permanent failures (404, bad bytes) are not retried; nor are
        // transient ones within a minute.
        for path in paths {
            assert!(h.posters.image(&key(path)).is_none());
        }
        std::thread::sleep(Duration::from_millis(100));
        let seen = server.seen().len();
        assert_eq!(seen, 4, "no second request");
    }

    #[test]
    fn a_damaged_cache_file_is_replaced() {
        let server = image_server();
        let dir = TestDir::new("poster-damaged");
        let posters_dir = dir.0.join("posters");
        fs::create_dir_all(&posters_dir).unwrap();
        fs::write(posters_dir.join("tmdb-w185-good.jpg"), b"half a jp").unwrap();
        let h = Harness::new(Some(posters_dir.clone()), &server);
        assert!(h.posters.image(&key("/good.jpg")).is_none());
        h.settle(1);
        assert_eq!(h.posters.stats().downloads, 1);
        assert!(decode(&fs::read(posters_dir.join("tmdb-w185-good.jpg")).unwrap()).is_ok());
    }

    #[test]
    fn without_a_cache_folder_posters_still_show() {
        let server = image_server();
        let h = Harness::new(None, &server);
        assert!(h.posters.image(&key("/other.jpg")).is_none());
        h.settle(1);
        assert!(h.posters.image(&key("/other.jpg")).is_some());
    }
}
