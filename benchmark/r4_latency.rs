//! Opt-in R4 harness. Product operations are reused, not reimplemented.
use super::*;
use slint::{ComponentHandle, Model};
use std::collections::VecDeque;
use std::error::Error;
use std::fs::File;
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Samples(BufWriter<File>);

impl Samples {
    fn new(path: &Path) -> Result<Self> {
        let mut file = File::create_new(path)?;
        writeln!(
            file,
            "category,case,sample,elapsed_ns,rows,hits,misses,evictions,selected_id"
        )?;
        Ok(Self(BufWriter::new(file)))
    }

    fn record(
        &mut self,
        category: &str,
        case: &str,
        sample: usize,
        elapsed: Duration,
        rows: usize,
        counters: (u64, u64, u64, i32),
    ) -> Result<()> {
        let (hits, misses, evictions, selected) = counters;
        writeln!(
            self.0,
            "{category},{case},{sample},{},{rows},{hits},{misses},{evictions},{selected}",
            elapsed.as_nanos()
        )?;
        self.0.flush()?;
        Ok(())
    }
}

pub(crate) fn run() -> Result<()> {
    if cfg!(debug_assertions) {
        return Err("R4 requires release mode".into());
    }
    let mode = std::env::args()
        .nth(2)
        .ok_or("missing sqlite/posters/ui mode")?;
    let output = PathBuf::from(std::env::args().nth(3).ok_or("missing output CSV")?);
    let mut samples = Samples::new(&output)?;
    for sample in 0..1000 {
        let start = Instant::now();
        black_box(());
        let elapsed = start.elapsed();
        samples.record("overhead", "timer_pair", sample, elapsed, 0, (0, 0, 0, -1))?;
    }
    match mode.as_str() {
        "sqlite" => sqlite(&mut samples)?,
        "posters" => posters(&mut samples)?,
        "ui" => ui(samples, &output)?,
        _ => return Err("unknown measurement mode".into()),
    }
    std::fs::write(
        output.with_extension("complete"),
        "completed successfully\n",
    )?;
    Ok(())
}

fn expected<'a>(oracle: &'a [MediaItem], query: &str) -> Vec<&'a MediaItem> {
    library::search(oracle, query)
        .iter()
        .map(|&row| &oracle[row])
        .collect()
}

fn verify_rows(rows: &[MediaItem], oracle: &[MediaItem], query: &str) {
    assert_eq!(rows.iter().collect::<Vec<_>>(), expected(oracle, query));
}

fn sqlite(samples: &mut Samples) -> Result<()> {
    let path = database_path()?;
    if !path.is_file() {
        return Err("frozen existing database missing".into());
    }
    let oracle = library::generate_library();
    for sample in 0..50 {
        let start = Instant::now();
        let conn = db::open(&path)?;
        let elapsed = start.elapsed();
        verify_rows(&db::search(&conn, "")?, &oracle, "");
        samples.record(
            "sqlite",
            "existing_open",
            sample,
            elapsed,
            1000,
            (0, 0, 0, -1),
        )?;
    }
    let conn = db::open(&path)?;
    // First query on this connection is retained, including statement preparation.
    for (label, query) in [
        ("all", ""),
        ("HARBOR", "HARBOR"),
        ("NÖRDLICHE", "NÖRDLICHE"),
        ("zzzz", "zzzz"),
    ] {
        for sample in 0..50 {
            let start = Instant::now();
            let rows = db::search(black_box(&conn), black_box(query))?;
            let elapsed = start.elapsed();
            verify_rows(&rows, &oracle, query);
            samples.record("sqlite", label, sample, elapsed, rows.len(), (0, 0, 0, -1))?;
        }
    }
    Ok(())
}

fn load(number: u32) -> Image {
    let image = Image::load_from_path(&poster_dir().join(poster::file_name(number)))
        .expect("frozen valid poster must decode");
    assert_eq!((image.size().width, image.size().height), (240, 360));
    image
}

fn posters(samples: &mut Samples) -> Result<()> {
    for number in 1..=100 {
        let path = poster_dir().join(poster::file_name(number));
        let start = Instant::now();
        let image = Image::load_from_path(black_box(&path))?;
        let elapsed = start.elapsed();
        assert_eq!((image.size().width, image.size().height), (240, 360));
        samples.record(
            "poster",
            "first_path_decode",
            number as usize - 1,
            elapsed,
            1,
            (0, 1, 0, -1),
        )?;
    }
    // 100 RGB buffers exceed Slint's 5 MiB LRU. No handle is retained here.
    for number in 1..=100 {
        drop(load(number));
    }
    for sample in 0..500 {
        let path = poster_dir().join(poster::file_name(1 + sample as u32 % 100));
        let start = Instant::now();
        let image = Image::load_from_path(black_box(&path))?;
        let elapsed = start.elapsed();
        black_box(&image);
        samples.record("poster", "forced_decode", sample, elapsed, 1, (0, 1, 0, -1))?;
    }
    let path = poster_dir().join(poster::file_name(1));
    drop(load(1)); // Prime before the first measured Slint hit.
    for sample in 0..500 {
        let start = Instant::now();
        let image = Image::load_from_path(black_box(&path))?;
        let elapsed = start.elapsed();
        black_box(&image);
        samples.record(
            "poster",
            "slint_internal_hit",
            sample,
            elapsed,
            1,
            (0, 0, 0, -1),
        )?;
    }
    let mut cache = PosterCache::new(poster::BUDGET_BYTES, slint_poster_loader(poster_dir()));
    for number in 1..=36 {
        drop(cache.get(number));
    }
    for sample in 0..500 {
        let before = cache.stats;
        let start = Instant::now();
        let image = cache.get(black_box(1 + sample as u32 % 36));
        let elapsed = start.elapsed();
        assert!(image.is_some());
        assert_eq!(cache.stats.misses, before.misses);
        samples.record(
            "poster",
            "explicit_hit",
            sample,
            elapsed,
            1,
            (cache.stats.hits - before.hits, 0, 0, -1),
        )?;
    }
    for sample in 0..50 {
        // End each 100-path sweep at 100: paths 1..9 miss both LRUs.
        for number in 1..=100 {
            drop(cache.get(number));
        }
        let before = cache.stats;
        let start = Instant::now();
        let images: Vec<_> = (1..=9).map(|number| cache.get(number)).collect();
        let elapsed = start.elapsed();
        assert!(images.iter().all(Option::is_some));
        assert_eq!(cache.stats.misses - before.misses, 9);
        assert_eq!(cache.stats.failures, 0);
        samples.record(
            "poster",
            "nine_new_images",
            sample,
            elapsed,
            9,
            (
                cache.stats.hits - before.hits,
                cache.stats.misses - before.misses,
                cache.stats.evictions - before.evictions,
                -1,
            ),
        )?;
    }
    Ok(())
}

enum Action {
    Wait,
    Search(&'static str, &'static str, usize),
    Query(&'static str),
    Navigate(i32),
    Select(&'static str, usize, i32),
}

fn ui(mut samples: Samples, output: &Path) -> Result<()> {
    let window = AppWindow::new()?;
    let view = Rc::new(open_library()?);
    connect(&window, view.clone());
    let oracle = library::generate_library();
    let mut actions = VecDeque::new();
    actions.extend((0..125).map(|_| Action::Wait));
    for sample in 0..50 {
        for (label, query) in [
            ("restore", ""),
            ("HARBOR", "HARBOR"),
            ("NÖRDLICHE", "NÖRDLICHE"),
            ("zzzz", "zzzz"),
            ("clear", ""),
        ] {
            actions.push_back(Action::Search(label, query, sample));
        }
    }
    actions.push_back(Action::Navigate(0));
    for sample in 0..50 {
        actions.push_back(Action::Select("cached", sample, 0));
    }
    for sample in 0..50 {
        actions.push_back(Action::Select("explicit_miss", sample, 0));
    }
    // Programmatic traversal prepares this latency cohort only. It is not an
    // A-F/soak run and does not prove keyboard handling or smooth movement.
    actions.push_back(Action::Navigate(0));
    for step in 1..=130 {
        actions.push_back(Action::Navigate((step * 8).min(999)));
    }
    actions.push_back(Action::Navigate(999));
    for step in 1..=130 {
        actions.push_back(Action::Navigate((999 - step * 8).max(0)));
    }
    actions.push_back(Action::Navigate(0));
    actions.push_back(Action::Navigate(999));
    for sample in 0..50 {
        actions.push_back(Action::Select(
            "after_navigation",
            sample,
            950 + sample as i32,
        ));
    }
    actions.push_back(Action::Query("HARBOR"));
    for sample in 0..50 {
        actions.push_back(Action::Select("after_HARBOR", sample, sample as i32 % 25));
    }
    let weak = window.as_weak();
    let outcome = Rc::new(RefCell::new(None));
    let result = outcome.clone();
    let geometry_path = output.with_extension("geometry.txt");
    let mut geometry_recorded = false;
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(40), move || {
        let Some(window) = weak.upgrade() else { return; };
        let mut operation = || -> Result<bool> {
            let Some(action) = actions.pop_front() else { return Ok(true); };
            if matches!(action, Action::Wait) { return Ok(false); }
            if !geometry_recorded {
                let size = window.window().size();
                let scale = window.window().scale_factor();
                std::fs::write(&geometry_path, format!("physical_width={}\nphysical_height={}\nscale_factor={}\nlogical_width={}\nlogical_height={}\n", size.width, size.height, scale, size.width as f32/scale, size.height as f32/scale))?;
                if (size.width as f32/scale - 1280.0).abs() > 1.0 || (size.height as f32/scale - 800.0).abs() > 1.0 {
                    return Err("window geometry differs from frozen 1280x800 workload".into());
                }
                geometry_recorded = true;
            }
            match action {
                Action::Wait => (),
                Action::Query(query) => { window.invoke_query_changed(query.into()); verify_rows(&view.results.borrow(), &oracle, query); }
                Action::Navigate(row) => { window.invoke_row_selected(row); window.invoke_reveal_row(row); }
                Action::Search(label, query, sample) => {
                    let wanted = crate::view::reselect(&expected(&oracle, query).into_iter().cloned().collect::<Vec<_>>(), |item| item.id.into(), view.selected.get());
                    let query_value: slint::SharedString = query.into();
                    let before = view.library.posters.borrow().stats;
                    let start = Instant::now();
                    window.invoke_query_changed(query_value);
                    let elapsed = start.elapsed();
                    verify_rows(&view.results.borrow(), &oracle, query);
                    assert_eq!(view.selected.get(), wanted);
                    verify_detail(&window, &view);
                    let after = view.library.posters.borrow().stats;
                    samples.record("search", label, sample, elapsed, view.row_count(), (after.hits-before.hits, after.misses-before.misses, after.evictions-before.evictions, window.get_selected_id()))?;
                }
                Action::Select(label, sample, row) => {
                    if label == "explicit_miss" {
                        for number in 2..=100 { drop(view.library.posters.borrow_mut().get(number)); }
                    }
                    let before = view.library.posters.borrow().stats;
                    let start = Instant::now();
                    window.invoke_row_selected(row);
                    let elapsed = start.elapsed();
                    let after = view.library.posters.borrow().stats;
                    verify_detail(&window, &view);
                    assert_eq!(window.get_selected_row(), row);
                    if label == "explicit_miss" { assert_eq!(after.misses-before.misses, 1); }
                    if label == "cached" { assert_eq!(after.misses-before.misses, 0); assert_eq!(after.hits-before.hits, 1); }
                    samples.record("selection", label, sample, elapsed, view.row_count(), (after.hits-before.hits, after.misses-before.misses, after.evictions-before.evictions, window.get_selected_id()))?;
                    window.invoke_reveal_row(row);
                }
            }
            assert!(window.get_error().is_empty());
            assert_eq!(view.library.posters.borrow().stats.failures, 0);
            Ok(false)
        };
        match operation() {
            Ok(false) => (),
            completed => {
                *result.borrow_mut() = Some(completed.map(|_| ()).map_err(|error| error.to_string()));
                slint::quit_event_loop().expect("active event loop");
            }
        }
    });
    window.run()?;
    timer.stop();
    outcome
        .borrow_mut()
        .take()
        .ok_or("window closed before measurements completed")?
        .map_err(Into::into)
}

fn verify_detail(window: &AppWindow, view: &LibraryView) {
    let rows = view.results.borrow();
    match view.selected.get() {
        Some(id) => {
            let item = rows
                .iter()
                .find(|item| i64::from(item.id) == id)
                .expect("selected id in results");
            let detail = window.get_detail();
            assert_eq!(detail.id, id as i32);
            assert_eq!(detail.title.as_str(), item.title);
            assert_eq!(detail.original_title.as_str(), item.original_title);
            assert_eq!(detail.overview.as_str(), item.overview);
            assert_eq!(detail.progress, item.progress.fraction());
            assert_eq!(
                detail.poster.path(),
                Some(
                    poster_dir()
                        .join(poster::file_name(poster::poster_number(item.id)))
                        .as_path()
                )
            );
        }
        None => {
            assert_eq!(window.get_selected_id(), -1);
            assert_eq!(window.get_selected_row(), -1);
        }
    }
}
