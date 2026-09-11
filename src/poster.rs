//! Poster pipeline: which synthetic poster a media item shows, and a small LRU
//! of loaded posters bounded by their estimated decoded size.
//!
//! No Slint types. The cache is generic over the loaded value, so it is tested
//! with plain values; `main.rs` plugs in `slint::Image`. See
//! `docs/adr/0003-poster-pipeline-and-bounded-cache.md`.

use std::collections::HashSet;
use std::fmt;
use std::time::{Duration, Instant};

/// Number of posters in the shared pool `benchmark/assets/posters`.
pub const POOL_SIZE: u32 = 100;

/// Cache budget in estimated decoded bytes. A 240×360 poster is estimated at
/// 240 × 360 × 4 = 345,600 bytes, so 12 MiB holds 36 posters: about four
/// screens of rows at the default window size (9 rows), or two when maximized
/// on a 1440p display, plus the detail poster. The pool of 100 is well above
/// 36, so a long scroll has to evict.
pub const BUDGET_BYTES: usize = 12 * 1024 * 1024;

/// The poster (1..=`POOL_SIZE`) for a stable local id: id 1 → 1, id 100 → 100,
/// id 101 → 1, id 237 → 37. A pure function of the id, so it is not stored in
/// the database.
pub fn poster_number(id: u32) -> u32 {
    id.saturating_sub(1) % POOL_SIZE + 1
}

pub fn file_name(number: u32) -> String {
    format!("poster-{number:03}.jpg")
}

/// Loads poster `number`, returning the value and its estimated decoded size
/// in bytes, or `None` if it cannot be loaded.
pub type Loader<T> = Box<dyn FnMut(u32) -> Option<(T, usize)>>;

/// Counters since the cache was created. `misses` counts loader calls.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Loads that failed. That poster is not retried (and so not logged again).
    pub failures: u64,
    /// Loads larger than the whole budget: returned to the caller, not cached.
    pub uncached: u64,
    pub load_total: Duration,
    pub load_max: Duration,
}

/// Least-recently-used cache of loaded posters, keyed by poster number, that
/// never holds more than `budget` estimated bytes.
///
/// Evicting an entry drops the cache's handle. The value is freed once no one
/// else (for example a visible row) still holds a clone.
pub struct PosterCache<T> {
    budget: usize,
    used: usize,
    // ponytail: linear scan and O(n) move-to-back; fine for the ~36 entries
    // the budget allows. Use a linked hash map if it grows to thousands.
    /// (poster number, value, estimated bytes), least recently used first.
    entries: Vec<(u32, T, usize)>,
    /// Posters whose load failed. At most `POOL_SIZE` numbers, never pixels.
    failed: HashSet<u32>,
    load: Loader<T>,
    pub stats: Stats,
}

impl<T: Clone> PosterCache<T> {
    pub fn new(budget: usize, load: Loader<T>) -> Self {
        Self {
            budget,
            used: 0,
            entries: Vec::new(),
            failed: HashSet::new(),
            load,
            stats: Stats::default(),
        }
    }

    /// Poster `number`, loading it on a miss. `None` if it cannot be loaded.
    pub fn get(&mut self, number: u32) -> Option<T> {
        if let Some(index) = self.entries.iter().position(|entry| entry.0 == number) {
            self.stats.hits += 1;
            let entry = self.entries.remove(index);
            let value = entry.1.clone();
            self.entries.push(entry);
            return Some(value);
        }
        if self.failed.contains(&number) {
            return None;
        }

        self.stats.misses += 1;
        let start = Instant::now();
        let loaded = (self.load)(number);
        let elapsed = start.elapsed();
        self.stats.load_total += elapsed;
        self.stats.load_max = self.stats.load_max.max(elapsed);

        let Some((value, bytes)) = loaded else {
            self.stats.failures += 1;
            self.failed.insert(number);
            return None;
        };
        if bytes > self.budget {
            // Caching it would mean evicting everything and still exceeding
            // the budget. Show it, but reload it next time.
            self.stats.uncached += 1;
            return Some(value);
        }
        // Terminates: with no entries left `used` is 0, and `bytes` fits.
        while self.used + bytes > self.budget {
            let (_, _, freed) = self.entries.remove(0);
            self.used -= freed;
            self.stats.evictions += 1;
        }
        self.used += bytes;
        self.entries.push((number, value.clone(), bytes));
        Some(value)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn used_bytes(&self) -> usize {
        self.used
    }
}

/// One-line summary for the debug dump (F12 in the list).
impl<T> fmt::Display for PosterCache<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mib = |bytes: usize| bytes as f64 / (1024.0 * 1024.0);
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        let s = &self.stats;
        let avg = if s.misses == 0 {
            Duration::ZERO
        } else {
            s.load_total / s.misses as u32
        };
        write!(
            f,
            "posters: entries={} est_bytes={} ({:.2}/{:.2} MiB) hits={} misses={} evictions={} failures={} uncached={} load_avg={:.2}ms load_max={:.2}ms",
            self.entries.len(),
            self.used,
            mib(self.used),
            mib(self.budget),
            s.hits,
            s.misses,
            s.evictions,
            s.failures,
            s.uncached,
            ms(avg),
            ms(s.load_max),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::{Rc, Weak};

    /// A cache of `Rc<u32>` values, each estimated at `size(number)` bytes,
    /// that records every loader call and keeps a weak handle to each
    /// loaded value.
    struct Harness {
        calls: Rc<RefCell<Vec<u32>>>,
        loaded: Rc<RefCell<Vec<Weak<u32>>>>,
    }

    fn cache(budget: usize, size: fn(u32) -> Option<usize>) -> (PosterCache<Rc<u32>>, Harness) {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let loaded = Rc::new(RefCell::new(Vec::new()));
        let harness = Harness {
            calls: calls.clone(),
            loaded: loaded.clone(),
        };
        let load = Box::new(move |number| {
            calls.borrow_mut().push(number);
            let value = Rc::new(number);
            loaded.borrow_mut().push(Rc::downgrade(&value));
            size(number).map(|bytes| (value, bytes))
        });
        (PosterCache::new(budget, load), harness)
    }

    fn keys<T>(cache: &PosterCache<T>) -> Vec<u32> {
        cache.entries.iter().map(|entry| entry.0).collect()
    }

    #[test]
    fn mapping_is_deterministic_and_covers_the_pool() {
        assert_eq!(poster_number(1), 1);
        assert_eq!(poster_number(100), 100);
        assert_eq!(poster_number(101), 1);
        assert_eq!(poster_number(237), 37);
        assert_eq!(poster_number(1000), 100);
        let used: HashSet<u32> = (1..=1000).map(poster_number).collect();
        assert_eq!(used, (1..=POOL_SIZE).collect());
        assert_eq!(file_name(7), "poster-007.jpg");
        assert_eq!(file_name(100), "poster-100.jpg");
    }

    #[test]
    fn miss_loads_once_then_hits() {
        let (mut cache, h) = cache(100, |_| Some(10));
        assert_eq!((cache.len(), cache.used_bytes()), (0, 0), "starts empty");
        assert_eq!(cache.stats, Stats::default());

        assert_eq!(cache.get(3).as_deref(), Some(&3));
        assert_eq!(
            (cache.stats.hits, cache.stats.misses),
            (0, 1),
            "first access misses"
        );
        for _ in 0..5 {
            assert_eq!(cache.get(3).as_deref(), Some(&3));
        }
        assert_eq!((cache.stats.hits, cache.stats.misses), (5, 1));
        assert_eq!(*h.calls.borrow(), [3], "hits never call the loader");
        assert_eq!((cache.len(), cache.used_bytes()), (1, 10));
    }

    #[test]
    fn evicts_least_recently_used_and_stays_within_budget() {
        let (mut cache, h) = cache(30, |_| Some(10));
        for n in [1, 2, 3] {
            cache.get(n);
        }
        cache.get(1); // 1 becomes most recent; 2 is now the oldest.
        cache.get(4);
        assert_eq!(keys(&cache), [3, 1, 4]);
        assert_eq!(cache.stats.evictions, 1);

        cache.get(2); // evicted earlier: reloads, evicting 3.
        assert_eq!(keys(&cache), [1, 4, 2]);
        assert_eq!(*h.calls.borrow(), [1, 2, 3, 4, 2]);

        // Sequential churn over a pool bigger than the cache: every step
        // stays within the budget.
        for n in (1..=50).cycle().take(500) {
            cache.get(n);
            assert!(cache.used_bytes() <= 30);
            assert!(cache.len() <= 3);
        }
    }

    #[test]
    fn mixed_sizes_evict_until_the_new_entry_fits() {
        let (mut cache, _) = cache(100, |n| Some(n as usize));
        for n in [40, 30, 20] {
            cache.get(n);
        }
        assert_eq!(cache.used_bytes(), 90);
        cache.get(60); // evicting 40 is not enough; 30 goes too.
        assert_eq!(keys(&cache), [20, 60]);
        assert_eq!((cache.used_bytes(), cache.stats.evictions), (80, 2));
        cache.get(20);
        cache.get(20);
        assert_eq!(cache.used_bytes(), 80, "hits do not add bytes");
    }

    #[test]
    fn value_larger_than_the_budget_is_returned_but_not_cached() {
        let (mut cache, h) = cache(100, |n| Some(if n == 9 { 101 } else { 10 }));
        cache.get(1);
        assert_eq!(cache.get(9).as_deref(), Some(&9), "still shown");
        assert_eq!(keys(&cache), [1], "nothing evicted for it");
        assert_eq!((cache.stats.uncached, cache.stats.evictions), (1, 0));
        cache.get(9);
        assert_eq!(*h.calls.borrow(), [1, 9, 9], "reloaded each time");
    }

    #[test]
    fn failed_load_is_not_cached_and_not_retried() {
        let (mut cache, h) = cache(100, |n| (n != 5).then_some(10));
        assert_eq!(cache.get(5), None);
        assert_eq!(cache.get(5), None);
        assert_eq!(*h.calls.borrow(), [5], "no retry, so no repeated error log");
        assert_eq!((cache.len(), cache.used_bytes()), (0, 0));
        assert_eq!((cache.stats.misses, cache.stats.failures), (1, 1));
        assert_eq!(
            cache.get(6).as_deref(),
            Some(&6),
            "other posters still load"
        );
    }

    #[test]
    fn eviction_releases_the_value_once_no_one_else_holds_it() {
        let (mut cache, h) = cache(20, |_| Some(10));
        let held = cache.get(1); // like a visible row keeping its image
        drop(cache.get(2));
        cache.get(3);
        cache.get(4); // evicts 1 and 2
        let weak = |i: usize| h.loaded.borrow()[i].clone();
        assert!(
            weak(1).upgrade().is_none(),
            "evicted and unreferenced: freed"
        );
        assert!(
            weak(0).upgrade().is_some(),
            "evicted but still held by a row"
        );
        drop(held);
        assert!(weak(0).upgrade().is_none());
        assert!(weak(2).upgrade().is_some(), "still cached");
    }
}
