//! Benchmark fixture dataset: the deterministic 1,000-record fake library
//! (R1) and its in-memory reference search. Never part of a production build.
//!
//! Plain Rust types with no Slint dependency; `fixture/mod.rs` maps them to
//! the generated UI structs.

/// Size of the shared benchmark dataset (`BENCHMARK_SPEC.md`).
pub const LIBRARY_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Movie,
    Tv,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Movie => "Movie",
            MediaKind::Tv => "TV",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Planned,
    Watched,
    Episodes { watched: u16, total: u16 },
}

impl Progress {
    pub fn label(self) -> String {
        match self {
            Progress::Planned => "Planned".into(),
            Progress::Watched => "Watched".into(),
            Progress::Episodes { watched: 0, total } => format!("Not started · {total} episodes"),
            Progress::Episodes { watched, total } if watched >= total => {
                format!("Completed · {total} episodes")
            }
            Progress::Episodes { watched, total } => format!("{watched} of {total} episodes"),
        }
    }

    /// Completion in `0.0..=1.0`.
    pub fn fraction(self) -> f32 {
        match self {
            Progress::Planned | Progress::Episodes { total: 0, .. } => 0.0,
            Progress::Watched => 1.0,
            Progress::Episodes { watched, total } => {
                (f32::from(watched) / f32::from(total)).min(1.0)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaItem {
    /// Stable local id, `1..=LIBRARY_SIZE`, equal to dataset position + 1.
    pub id: u32,
    pub title: String,
    pub original_title: String,
    pub year: u16,
    pub kind: MediaKind,
    pub progress: Progress,
    pub overview: String,
    /// Lowercased title and original title, built once so a search does not
    /// allocate per record.
    search_text: String,
}

impl MediaItem {
    pub fn new(
        id: u32,
        title: String,
        original_title: String,
        year: u16,
        kind: MediaKind,
        progress: Progress,
        overview: String,
    ) -> Self {
        let search_text = format!("{title}\n{original_title}").to_lowercase();
        Self {
            id,
            title,
            original_title,
            year,
            kind,
            progress,
            overview,
            search_text,
        }
    }

    /// The lowercased search key. The SQLite seed stores it, so SQL search
    /// folds case exactly like `search` does.
    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    pub fn initials(&self) -> String {
        crate::library::initials(&self.title)
    }
}

/// Records 1–10: the recognizable R0 titles. Overviews are placeholder text
/// written for this spike, not provider metadata.
#[rustfmt::skip]
const SEEDS: [(&str, &str, u16, MediaKind, Progress, &str); 10] = {
    use MediaKind::{Movie, Tv};
    use Progress::{Episodes, Planned, Watched};
    [
        ("Severance", "Severance", 2022, Tv, Episodes { watched: 12, total: 19 },
            "Office workers whose memories are surgically split between work and home start to question what their employer is really doing."),
        ("Dark", "Dark", 2017, Tv, Episodes { watched: 26, total: 26 },
            "A missing child in a small German town exposes a knot of family secrets that spans several generations."),
        ("Dune: Part Two", "Dune : Deuxième partie", 2024, Movie, Watched,
            "An exiled heir joins the desert people of a hostile planet and is pulled toward a war he has foreseen."),
        ("The Bear", "The Bear", 2022, Tv, Episodes { watched: 14, total: 28 },
            "A fine-dining chef returns home to run his late brother's chaotic sandwich shop."),
        ("Arrival", "Premier contact", 2016, Movie, Watched,
            "A linguist is recruited to communicate with visitors whose language changes how she experiences time."),
        ("Shōgun", "Shogun", 2024, Tv, Episodes { watched: 0, total: 10 },
            "A shipwrecked navigator becomes entangled in the power struggle of feudal Japan."),
        ("Blade Runner 2049", "Blade Runner 2049", 2017, Movie, Planned,
            "A replicant hunter uncovers a buried secret that could upend what is left of society."),
        ("The Last of Us", "The Last of Us", 2023, Tv, Episodes { watched: 5, total: 16 },
            "A hardened survivor escorts a teenager across a collapsed America."),
        ("Past Lives", "Past Lives", 2023, Movie, Planned,
            "Two childhood friends reconnect across decades and continents."),
        ("Andor", "Andor", 2022, Tv, Episodes { watched: 24, total: 24 },
            "A drifting thief is slowly drawn into an uprising against an empire."),
    ]
};

/// Title words as (English title, German "original title") pairs. 25 × 40 =
/// 1,000 distinct combinations, enough for every generated record.
#[rustfmt::skip]
const ADJECTIVES: [(&str, &str); 25] = [
    ("Silent", "Stille"), ("Crimson", "Rote"), ("Hollow", "Hohle"), ("Golden", "Goldene"),
    ("Broken", "Zerbrochene"), ("Distant", "Ferne"), ("Frozen", "Gefrorene"),
    ("Hidden", "Verborgene"), ("Last", "Letzte"), ("Wild", "Wilde"), ("Burning", "Brennende"),
    ("Northern", "Nördliche"), ("Endless", "Endlose"), ("Lost", "Verlorene"), ("Iron", "Eiserne"),
    ("Secret", "Geheime"), ("Bright", "Helle"), ("Pale", "Blasse"), ("Restless", "Ruhelose"),
    ("Sunken", "Versunkene"), ("Electric", "Elektrische"), ("Wandering", "Wandernde"),
    ("Glass", "Gläserne"), ("Hungry", "Hungrige"), ("Velvet", "Samtene"),
];

#[rustfmt::skip]
const NOUNS: [(&str, &str); 40] = [
    ("Harbor", "Hafen"), ("Garden", "Garten"), ("River", "Fluss"), ("Crown", "Krone"),
    ("Mountain", "Berg"), ("Forest", "Wald"), ("Mirror", "Spiegel"), ("Tower", "Turm"),
    ("Island", "Insel"), ("Kingdom", "Königreich"), ("Empire", "Reich"), ("Frontier", "Grenze"),
    ("Lighthouse", "Leuchtturm"), ("Orchard", "Obstgarten"), ("Voyage", "Reise"),
    ("Letter", "Brief"), ("Machine", "Maschine"), ("Winter", "Winter"), ("Summer", "Sommer"),
    ("Circus", "Zirkus"), ("Signal", "Zeichen"), ("Theory", "Theorie"),
    ("Company", "Gesellschaft"), ("Protocol", "Protokoll"), ("Valley", "Tal"),
    ("Bridge", "Brücke"), ("Engine", "Motor"), ("Archive", "Archiv"), ("Horizon", "Horizont"),
    ("Canyon", "Schlucht"), ("Station", "Bahnhof"), ("Colony", "Kolonie"),
    ("Frequency", "Frequenz"), ("Hunter", "Jäger"), ("Pilgrim", "Pilger"), ("Witness", "Zeuge"),
    ("Stranger", "Fremde"), ("Parade", "Umzug"), ("Satellite", "Satellit"),
    ("Monument", "Denkmal"),
];

#[rustfmt::skip]
const GENRES: [&str; 8] = [
    "slow-burn mystery", "sprawling family drama", "dry workplace comedy",
    "quiet science-fiction", "tense survival thriller", "wistful romance", "offbeat crime",
    "sweeping historical",
];

#[rustfmt::skip]
const SUBJECTS: [&str; 10] = [
    "two estranged siblings", "a retired cartographer", "a crew of night-shift engineers",
    "an archivist with a secret", "rival chefs", "a stubborn small-town mayor",
    "a band on its final tour", "a detective who cannot sleep",
    "a family of lighthouse keepers", "an unlikely pair of forgers",
];

#[rustfmt::skip]
const SETTINGS: [&str; 8] = [
    "on a remote northern coast", "in a city that never floods",
    "aboard a failing space station", "in a mountain village", "during one long winter",
    "in the near future", "across three decades", "in a crumbling seaside hotel",
];

/// Builds the 1,000-record dataset: the R0 seed titles, then generated
/// records. Pure arithmetic on the record index, no RNG, so every call (and
/// every launch) yields the same records in the same order.
pub fn generate_library() -> Vec<MediaItem> {
    let seeds = SEEDS
        .iter()
        .map(|&(title, original, year, kind, progress, overview)| {
            (
                title.to_owned(),
                original.to_owned(),
                year,
                kind,
                progress,
                overview.to_owned(),
            )
        });
    let generated = (0..LIBRARY_SIZE - SEEDS.len()).map(generated_record);
    seeds
        .chain(generated)
        .zip(1u32..)
        .map(|((title, original, year, kind, progress, overview), id)| {
            MediaItem::new(id, title, original, year, kind, progress, overview)
        })
        .collect()
}

type RecordFields = (String, String, u16, MediaKind, Progress, String);

fn generated_record(k: usize) -> RecordFields {
    // 389 is coprime with 1,000, so `k -> combo` is a bijection over the 1,000
    // word combinations: titles are unique and neighbours look unrelated.
    let combo = (k * 389 + 17) % (ADJECTIVES.len() * NOUNS.len());
    let (adj, adj_de) = ADJECTIVES[combo % ADJECTIVES.len()];
    let (noun, noun_de) = NOUNS[combo / ADJECTIVES.len()];

    let kind = if (k * 7) % 10 < 4 {
        MediaKind::Tv
    } else {
        MediaKind::Movie
    };
    let progress = match kind {
        MediaKind::Movie if (k * 11) % 5 < 2 => Progress::Watched,
        MediaKind::Movie => Progress::Planned,
        MediaKind::Tv => {
            let total = 6 + (k % 7) as u16 * 4;
            let watched = ((k * 13) % (usize::from(total) + 1)) as u16;
            Progress::Episodes { watched, total }
        }
    };
    let format = if kind == MediaKind::Tv {
        "series"
    } else {
        "film"
    };
    let overview = format!(
        "A {} {format} about {}, set {}.",
        GENRES[(k * 5 + 1) % GENRES.len()],
        SUBJECTS[(k * 3) % SUBJECTS.len()],
        SETTINGS[(k * 7 + 2) % SETTINGS.len()],
    );

    (
        format!("{adj} {noun}"),
        format!("{adj_de} {noun_de}"),
        1970 + ((k * 47) % 56) as u16,
        kind,
        progress,
        overview,
    )
}

/// Indices into `items` whose title or original title contains `query`,
/// case-insensitively, in dataset order. A blank query matches everything.
///
/// The R1 in-memory search. Since R2 the app searches with `db::search`; this
/// stays as the reference oracle its tests compare against.
#[cfg(any(test, feature = "r4-measurement"))]
pub fn search(items: &[MediaItem], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.search_text.contains(&query))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn ids(items: &[MediaItem], results: &[usize]) -> Vec<u32> {
        results.iter().map(|&index| items[index].id).collect()
    }

    #[test]
    fn generates_exactly_1000_records_with_unique_sequential_ids() {
        let items = generate_library();
        assert_eq!(items.len(), 1000);
        let unique: HashSet<_> = items.iter().map(|i| i.id).collect();
        assert_eq!(unique.len(), 1000);
        assert!(items.iter().zip(1u32..).all(|(item, id)| item.id == id));
        let titles: HashSet<_> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles.len(), 1000, "titles are unique");
    }

    #[test]
    fn generation_is_deterministic_and_pinned() {
        let items = generate_library();
        assert_eq!(items, generate_library());
        // Pinned samples: changing these changes the shared benchmark dataset.
        assert_eq!(items[0].title, "Severance");
        assert_eq!(items[10].title, "Pale Harbor");
        assert_eq!(items[10].original_title, "Blasse Hafen");
        assert_eq!(items[999].title, "Lost Canyon");
        assert_eq!(items[999].year, 1973);
    }

    #[test]
    fn mixes_movies_and_tv_with_plausible_fields() {
        let items = generate_library();
        let tv = items.iter().filter(|i| i.kind == MediaKind::Tv).count();
        assert!((300..=500).contains(&tv), "{tv} TV records");
        assert!(items.iter().all(|i| (1970..=2025).contains(&i.year)));
        assert!(
            items
                .iter()
                .all(|i| !i.overview.is_empty() && !i.initials().is_empty())
        );
    }

    #[test]
    fn blank_query_returns_all_records_in_order() {
        let items = generate_library();
        let all: Vec<usize> = (0..items.len()).collect();
        assert_eq!(search(&items, ""), all);
        assert_eq!(search(&items, "   "), all);
    }

    #[test]
    fn title_search_is_case_insensitive() {
        let items = generate_library();
        assert_eq!(ids(&items, &search(&items, "SEVERANCE")), [1]);
        assert_eq!(ids(&items, &search(&items, "dArK")), [2]);
        assert_eq!(search(&items, "harbor"), search(&items, "HaRbOr"));
    }

    #[test]
    fn original_title_search_matches() {
        let items = generate_library();
        assert_eq!(ids(&items, &search(&items, "premier")), [5]);
        assert_eq!(ids(&items, &search(&items, "shogun")), [6]);
        let hafen = search(&items, "HAFEN");
        assert!(!hafen.is_empty());
        assert_eq!(
            hafen,
            search(&items, "harbor"),
            "Hafen is Harbor's original title"
        );
    }

    #[test]
    fn unmatched_query_returns_nothing() {
        let items = generate_library();
        assert!(search(&items, "zzzz").is_empty());
    }

    #[test]
    fn results_are_ordered_and_match_a_brute_force_filter() {
        let items = generate_library();
        for query in ["an", "Bridge", "ö", "the "] {
            let results = search(&items, query);
            assert!(
                results.windows(2).all(|w| w[0] < w[1]),
                "{query}: dataset order"
            );
            let q = query.trim().to_lowercase();
            let expected: Vec<u32> = items
                .iter()
                .filter(|i| {
                    i.title.to_lowercase().contains(&q)
                        || i.original_title.to_lowercase().contains(&q)
                })
                .map(|i| i.id)
                .collect();
            assert_eq!(ids(&items, &results), expected, "{query}");
        }
    }

    #[test]
    fn reselect_keeps_visible_selection_else_first_result_else_none() {
        let items = generate_library();
        let found = |query| -> Vec<MediaItem> {
            search(&items, query)
                .into_iter()
                .map(|index| items[index].clone())
                .collect()
        };
        let reselect = |results: &[MediaItem], selected| {
            crate::view::reselect(results, |item| item.id.into(), selected)
        };
        assert_eq!(reselect(&found(""), Some(3)), Some(3));
        assert_eq!(reselect(&found("dune"), Some(3)), Some(3));
        let harbors = found("harbor");
        let first = Some(i64::from(harbors[0].id));
        assert_eq!(reselect(&harbors, Some(3)), first);
        assert_eq!(reselect(&harbors, None), first);
        assert_eq!(reselect(&found("zzzz"), Some(3)), None);
    }

    #[test]
    fn initials_skip_punctuation_and_handle_non_ascii() {
        let initials = |title: &str| {
            MediaItem::new(
                0,
                title.into(),
                String::new(),
                2000,
                MediaKind::Movie,
                Progress::Planned,
                String::new(),
            )
            .initials()
        };
        assert_eq!(initials("Dune: Part Two"), "DP");
        assert_eq!(initials("Blade Runner 2049"), "BR");
        assert_eq!(initials("Shōgun"), "S");
        assert_eq!(initials("dark"), "D");
        assert_eq!(initials(""), "");
    }

    #[test]
    fn progress_labels_and_fractions() {
        let ep = |watched, total| Progress::Episodes { watched, total };
        assert_eq!(Progress::Planned.label(), "Planned");
        assert_eq!(ep(0, 10).label(), "Not started · 10 episodes");
        assert_eq!(ep(5, 16).label(), "5 of 16 episodes");
        assert_eq!(ep(24, 24).label(), "Completed · 24 episodes");
        assert_eq!(ep(0, 0).fraction(), 0.0);
        assert_eq!(ep(8, 16).fraction(), 0.5);
        assert_eq!(ep(20, 16).fraction(), 1.0);
        assert_eq!(Progress::Watched.fraction(), 1.0);
    }
}
