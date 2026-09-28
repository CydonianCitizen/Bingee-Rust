//! The Statistics page: reads `statistics::load` each time the page opens or
//! the range changes, and turns it into prepared text and bar lengths. Read
//! only, synchronous on the UI thread (ADR-0020 has the measurements), local
//! only.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::database::SharedDb;
use crate::detail::duration;
use crate::diagnostics::Log;
use crate::metadata::Clock;
use crate::statistics::{self, Range, Statistics};
use crate::{AppWindow, StatBar, StatCard, StatisticsView};

/// How many genres are listed; the rest are counted in the note.
const GENRES_SHOWN: usize = 8;

pub fn start(window: &AppWindow, db: SharedDb, clock: Clock, log: Arc<Log>) {
    let range = Rc::new(Cell::new(Range::default()));
    let load = {
        let window = window.as_weak();
        let range = range.clone();
        move || {
            let Some(window) = window.upgrade() else {
                return;
            };
            let range = range.get();
            let view = match db.with(|db| statistics::load(db, range, clock.now())) {
                Ok(stats) => view(&stats),
                Err(error) => {
                    // The error names the failure, never a watched title.
                    log.error(format_args!("Statistics: not read: {error}"));
                    StatisticsView {
                        state: "error".into(),
                        notice: error.message.into(),
                        ..StatisticsView::default()
                    }
                }
            };
            window.set_statistics(StatisticsView {
                range: Range::ALL.iter().position(|r| *r == range).unwrap_or(0) as i32,
                ..view
            });
        }
    };
    let load = Rc::new(load);
    window.on_statistics_opened({
        let load = load.clone();
        move || load()
    });
    window.on_statistics_range(move |index| {
        if let Some(chosen) = usize::try_from(index).ok().and_then(|i| Range::ALL.get(i)) {
            range.set(*chosen);
            load();
        }
    });
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(items)))
}

/// "1 watch", "2 watches".
fn count(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn minutes(total: u64) -> String {
    duration(u32::try_from(total).unwrap_or(u32::MAX))
}

fn card(label: &str, value: impl Into<SharedString>, detail: impl Into<SharedString>) -> StatCard {
    StatCard {
        label: label.into(),
        value: value.into(),
        detail: detail.into(),
    }
}

fn bars(items: impl Iterator<Item = (String, u32, String)>) -> Vec<StatBar> {
    let items: Vec<_> = items.collect();
    let max = items.iter().map(|(_, n, _)| *n).max().unwrap_or(0).max(1);
    items
        .into_iter()
        .map(|(label, n, value)| StatBar {
            label: label.into(),
            value: value.into(),
            fraction: n as f32 / max as f32,
        })
        .collect()
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A short bar label: every fifth day of the month ("5"), a month ("Sep"), a
/// year ("2026").
fn bucket_label(range: Range, index: usize, key: &str) -> String {
    match range {
        Range::Days30 if index % 5 == 4 => key
            .get(8..10)
            .map_or_else(String::new, |d| d.trim_start_matches('0').to_owned()),
        Range::Days30 => String::new(),
        Range::Months12 => key
            .get(5..7)
            .and_then(|m| m.parse::<usize>().ok())
            .and_then(|m| MONTHS.get(m.wrapping_sub(1)))
            .map_or_else(|| key.to_owned(), |m| (*m).to_owned()),
        Range::AllTime => key.to_owned(),
    }
}

fn view(stats: &Statistics) -> StatisticsView {
    let library = library_part(stats);
    if !stats.has_history {
        return StatisticsView {
            state: "empty".into(),
            ..library
        };
    }
    let (total, movies, episodes) = (stats.total(), stats.movies, stats.episodes);
    let period = match stats.range {
        Range::Days30 => "the last 30 days",
        Range::Months12 => "the last 12 months",
        Range::AllTime => "all time",
    };
    let runtime_detail = match (total.watches, total.unknown_runtime) {
        (0, _) => "No watches in this period".to_owned(),
        (_, 0) => "Every watch has a known runtime".to_owned(),
        (_, unknown) => format!(
            "Known runtimes only: {} not counted",
            count(
                unknown,
                "watch with no runtime is",
                "watches with no runtime are"
            )
        ),
    };
    let cards = vec![
        card("Watch time", minutes(total.known_minutes), runtime_detail),
        card(
            "Watches",
            total.watches.to_string(),
            "Movies and episodes, rewatches included",
        ),
        card(
            "Movie watches",
            movies.watches.to_string(),
            count(movies.titles, "different movie", "different movies"),
        ),
        card(
            "Episode watches",
            episodes.watches.to_string(),
            count(episodes.titles, "different episode", "different episodes"),
        ),
        card(
            "Rewatches",
            total.rewatches.to_string(),
            "Watches of a movie or episode seen before",
        ),
    ];

    let activity = bars(stats.activity.iter().enumerate().map(|(i, bucket)| {
        let value = match bucket.watches {
            0 => String::new(),
            n => n.to_string(),
        };
        (
            bucket_label(stats.range, i, &bucket.key),
            bucket.watches,
            value,
        )
    }));

    let line = |label: &str, v: statistics::Viewing| {
        format!(
            "{label} · {} · {}",
            minutes(v.known_minutes),
            count(v.watches, "watch", "watches")
        )
    };
    let movie_share = match total.known_minutes {
        0 => -1.0,
        known => movies.known_minutes as f32 / known as f32,
    };
    let split_note = match (total.known_minutes, total.unknown_runtime) {
        (0, 0) => String::new(),
        (0, _) => "No runtime is known for these watches.".to_owned(),
        (_, 0) => String::new(),
        (_, unknown) => format!(
            "{} with no known runtime {} not included.",
            count(unknown, "watch", "watches"),
            if unknown == 1 { "is" } else { "are" }
        ),
    };

    let named = stats.genres.iter().filter(|g| g.name.is_some()).count();
    let genres = bars(
        stats
            .genres
            .iter()
            .filter(|g| g.name.is_some())
            .take(GENRES_SHOWN)
            .chain(stats.genres.iter().filter(|g| g.name.is_none()))
            .map(|g| {
                let value = match g.known_minutes {
                    0 => count(g.watches, "watch", "watches"),
                    known => format!(
                        "{} · {}",
                        count(g.watches, "watch", "watches"),
                        minutes(known)
                    ),
                };
                let label = g.name.clone().unwrap_or_else(|| "No genre stored".into());
                (label, g.watches, value)
            }),
    );
    let mut genre_note = match total.watches {
        0 => "No watches in this period.".to_owned(),
        _ => "A watch counts toward every genre of its title, so genres add up to more than \
              your watches."
            .to_owned(),
    };
    if named > GENRES_SHOWN {
        genre_note.push_str(&format!(
            " {} not shown.",
            count((named - GENRES_SHOWN) as u32, "more genre", "more genres")
        ));
    }

    let unit = match stats.range {
        Range::Days30 => "day",
        Range::Months12 => "month",
        Range::AllTime => "year",
    };
    StatisticsView {
        state: "ready".into(),
        history_title: format!("VIEWING HISTORY · {}", period.to_uppercase()).into(),
        cards: model(cards),
        activity_title: format!("Watches per {unit}").into(),
        activity: model(activity),
        movie_share,
        movies_line: line("Movies", movies).into(),
        tv_line: line("TV episodes", episodes).into(),
        split_note: split_note.into(),
        genres: model(genres),
        genre_note: genre_note.into(),
        ..library
    }
}

/// The "In your Library now" part, which needs no watch history.
fn library_part(stats: &Statistics) -> StatisticsView {
    let now = &stats.library;
    let average = now.average_rating();
    let series_note = match now.series_incomplete_coverage {
        0 => String::new(),
        n => format!(
            "{} not have every regular season's episode list downloaded, so {} count as \
             complete yet.",
            count(n, "series does", "series do"),
            if n == 1 { "it cannot" } else { "they cannot" }
        ),
    };
    let series = bars(
        [
            ("Complete", now.series_complete),
            ("In progress", now.series_in_progress),
            ("Not started", now.series_not_started),
        ]
        .into_iter()
        .map(|(label, n)| (label.to_owned(), n, n.to_string())),
    );
    // Series bars are shares of all series, not of the largest group.
    let series = series
        .into_iter()
        .zip([
            now.series_complete,
            now.series_in_progress,
            now.series_not_started,
        ])
        .map(|(bar, n)| StatBar {
            fraction: n as f32 / now.series.max(1) as f32,
            ..bar
        })
        .collect();
    let ratings = bars((1..).zip(now.ratings).map(|(r, n): (u32, u32)| {
        (
            r.to_string(),
            n,
            if n == 0 { String::new() } else { n.to_string() },
        )
    }));
    StatisticsView {
        has_library: !now.is_empty(),
        library_cards: model(vec![
            card(
                "Average rating",
                average.map_or_else(|| "–".to_owned(), |a| format!("{a:.1}")),
                match now.rated() {
                    0 => "No rated titles yet".to_owned(),
                    n => format!(
                        "Over {} (unrated not counted)",
                        count(n, "rated title", "rated titles")
                    ),
                },
            ),
            card(
                "Movies watched",
                format!("{} of {}", now.movies_watched, now.movies),
                "Marked watched now",
            ),
            card(
                "Episodes watched",
                now.episodes_watched.to_string(),
                "Marked watched now, specials included",
            ),
            card(
                "Series complete",
                format!("{} of {}", now.series_complete, now.series),
                "Every regular episode watched",
            ),
        ]),
        series: model(series),
        series_note: series_note.into(),
        rating_summary: match now.rated() {
            0 => "your own 1–10 ratings; none yet".into(),
            n => format!("{} · 1 to 10", count(n, "rated title", "rated titles")).into(),
        },
        ratings: model(ratings),
        ..StatisticsView::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::library;
    use crate::statistics::tests::{DAY, NOW, film, show};
    use crate::tests::Headless;
    use crate::tracking::{delete_event, recent_history, set_rating, watch_episode, watch_movie};
    use slint::Model;

    fn cards(view: &StatisticsView) -> Vec<(String, String)> {
        view.cards
            .iter()
            .map(|c| (c.label.into(), c.value.into()))
            .collect()
    }

    #[test]
    fn the_statistics_page_reloads_on_open_and_shows_each_state() {
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let (clock, _) = Clock::fake(NOW);
        start(&app, db.clone(), clock, Arc::new(Log::stderr_only()));
        let reopen = |ui: &mut Headless| {
            app.set_page("library".into());
            ui.render();
            app.set_page("statistics".into());
            ui.render();
        };

        // A new user: the empty state, no wall of zeros.
        app.set_page("statistics".into());
        ui.render();
        let view = app.get_statistics();
        assert_eq!((view.state.as_str(), view.has_library), ("empty", false));
        assert_eq!(view.cards.row_count(), 0);

        // A watch made elsewhere appears when the page opens again.
        let movie = db.with(|db| Ok(film(db, 603, vec![]))).unwrap();
        let tv = db
            .with(|db| {
                let tv = show(db, 1, &[(1, 2, 2)], vec![]);
                watch_movie(db, movie, NOW - DAY, false)?;
                watch_movie(db, movie, NOW, true)?;
                watch_episode(db, tv, 1, 1, NOW, false)?;
                set_rating(db, movie, Some(8))?;
                Ok(tv)
            })
            .unwrap();
        reopen(&mut ui);
        let view = app.get_statistics();
        assert_eq!(view.state, "ready");
        assert_eq!(
            cards(&view),
            [
                ("Watch time".into(), "5 h 34 min".into()),
                ("Watches".into(), "3".into()),
                ("Movie watches".into(), "2".into()),
                ("Episode watches".into(), "1".into()),
                ("Rewatches".into(), "1".into()),
            ]
        );
        assert_eq!(view.range, 1);
        assert_eq!(view.activity.row_count(), 12);
        let library: Vec<String> = view.library_cards.iter().map(|c| c.value.into()).collect();
        assert_eq!(library, ["8.0", "1 of 1", "1", "0 of 1"]);

        // Ranges.
        app.invoke_statistics_range(0);
        let view = app.get_statistics();
        assert_eq!((view.range, view.activity.row_count()), (0, 30));
        app.invoke_statistics_range(2);
        assert_eq!(app.get_statistics().range, 2);
        ui.render();

        // Removing a history entry and the series from the Library.
        db.with(|db| {
            let newest = recent_history(db, 1)?.remove(0);
            delete_event(db, newest.event_id)?;
            library::remove(db, tv)
        })
        .unwrap();
        reopen(&mut ui);
        let view = app.get_statistics();
        assert_eq!(view.range, 2, "the range is kept");
        assert_eq!(cards(&view)[1].1, "2");
        assert_eq!(view.library_cards.row_data(3).unwrap().value, "0 of 0");

        // A failing query: an error state, not zeros.
        db.with(|db| {
            db.conn()
                .execute_batch("ALTER TABLE watch_events RENAME TO moved")
                .unwrap();
            Ok(())
        })
        .unwrap();
        reopen(&mut ui);
        let view = app.get_statistics();
        assert_eq!(view.state, "error");
        assert!(view.notice.contains("statistics"), "{}", view.notice);
        assert!(!view.has_library);
    }

    /// Informal R11 observation, not a `BENCHMARK_SPEC.md` run: the page end
    /// to end on the UI thread (queries, view model, layout and a software
    /// render) over the synthetic large history in a file database. Run with
    /// `cargo test --release informal_statistics_page_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_statistics_page_timings() {
        use crate::paths::TestDir;
        use crate::statistics::tests::large_history;
        use std::time::{Duration, Instant};

        let dir = TestDir::new("statistics-page-timings");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        large_history(&Database::open(&path, &log).unwrap());
        let db = SharedDb::default();
        db.set(Database::open(&path, &log).unwrap());
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let (clock, _) = Clock::fake(NOW);
        start(&app, db, clock, Arc::new(log));

        let first = Instant::now();
        app.set_page("statistics".into());
        ui.render();
        println!(
            "first open (cold statement cache) + render: {:.3} ms",
            first.elapsed().as_secs_f64() * 1e3
        );
        for (label, index) in [("30 days", 0), ("12 months", 1), ("all time", 2)] {
            let mut samples: Vec<Duration> = (0..20)
                .map(|run| {
                    // Alternate so every sample changes the page.
                    app.invoke_statistics_range(if run % 2 == 0 { index } else { (index + 1) % 3 });
                    ui.render();
                    let start = Instant::now();
                    app.invoke_statistics_range(index);
                    ui.render();
                    start.elapsed()
                })
                .collect();
            samples.sort();
            println!(
                "choose {label:<10} + render: median {:.3} ms, max {:.3} ms (20 runs)",
                samples[10].as_secs_f64() * 1e3,
                samples[19].as_secs_f64() * 1e3
            );
        }
    }
}
