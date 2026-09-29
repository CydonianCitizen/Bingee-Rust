//! Portable Bingee backup V1. These DTOs are the external contract, separate
//! from SQLite rows and domain types. Restore validates before starting its
//! one atomic replacement transaction.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{Row, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::APP_VERSION;
use crate::database::Database;
use crate::error::{AppError, ErrorKind};

pub const FORMAT: &str = "bingee-backup";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupV1 {
    pub format: String,
    pub format_version: u32,
    /// Unix seconds, UTC.
    pub created_at: i64,
    pub application_version: String,
    pub data: BackupData,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BackupData {
    pub media: Vec<Media>,
    pub external_refs: Vec<ExternalRef>,
    pub library: Vec<LibraryEntry>,
    pub genres: Vec<Genre>,
    pub media_genres: Vec<MediaGenre>,
    pub seasons: Vec<Season>,
    pub episodes: Vec<Episode>,
    pub media_tracking: Vec<MediaTracking>,
    pub episode_tracking: Vec<EpisodeTracking>,
    pub watch_events: Vec<WatchEvent>,
    #[serde(default)]
    pub settings: BackupSettings,
    #[serde(default)]
    pub release_events: Vec<ReleaseEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BackupSettings {
    pub automatic_refresh_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Media {
    pub id: i64,
    pub media_type: String,
    pub title: String,
    pub original_title: Option<String>,
    pub release_date: Option<String>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub metadata_updated_at: Option<i64>,
    pub status: Option<String>,
    pub tagline: Option<String>,
    pub last_air_date: Option<String>,
    pub season_count: Option<i64>,
    pub episode_count: Option<i64>,
    pub details_fetched_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalRef {
    pub media_id: i64,
    pub source: String,
    pub media_type: String,
    pub external_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryEntry {
    pub media_id: i64,
    pub added_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Genre {
    pub source: String,
    pub external_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaGenre {
    pub media_id: i64,
    pub source: String,
    pub external_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Season {
    pub media_id: i64,
    pub number: i64,
    pub external_id: Option<String>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub episode_count: Option<i64>,
    pub poster_path: Option<String>,
    pub episodes_fetched_at: Option<i64>,
    pub episodes_known: Option<i64>,
    pub metadata_updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Episode {
    pub media_id: i64,
    pub season: i64,
    pub number: i64,
    pub external_id: Option<String>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<i64>,
    pub still_path: Option<String>,
    pub metadata_updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaTracking {
    pub media_id: i64,
    pub media_type: String,
    pub watched_at: Option<i64>,
    pub rating: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EpisodeTracking {
    pub media_id: i64,
    pub season: i64,
    pub episode: i64,
    pub watched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WatchEvent {
    pub id: i64,
    pub media_id: i64,
    pub media_type: String,
    pub season: Option<i64>,
    pub episode: Option<i64>,
    pub watched_at: i64,
    pub runtime_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseEvent {
    pub id: i64,
    pub media_id: i64,
    pub season: i64,
    pub episode: i64,
    pub kind: String,
    pub discovered_at: i64,
    pub air_date: Option<String>,
    pub read_at: Option<i64>,
}

fn read_error(error: rusqlite::Error) -> AppError {
    AppError::database("Backup could not read Bingee data.", error)
}

fn write_error(error: rusqlite::Error) -> AppError {
    AppError::database(
        "Restore failed. Your original data was left unchanged.",
        error,
    )
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorKind::InvalidData, message)
}

fn rows<T>(
    db: &Database,
    sql: &str,
    map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, AppError> {
    let mut stmt = db.conn().prepare(sql).map_err(read_error)?;
    stmt.query_map([], map)
        .map_err(read_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(read_error)
}

/// Snapshot all durable Bingee data in stable order. No credentials or cache.
pub fn export(db: &Database, created_at: i64) -> Result<BackupV1, AppError> {
    let media = rows(
        db,
        "SELECT local_media_id, media_type, title, original_title,
        release_date, overview, poster_path, backdrop_path, runtime_minutes,
        metadata_updated_at, status, tagline, last_air_date, season_count,
        episode_count, details_fetched_at FROM media ORDER BY local_media_id",
        |r| {
            Ok(Media {
                id: r.get(0)?,
                media_type: r.get(1)?,
                title: r.get(2)?,
                original_title: r.get(3)?,
                release_date: r.get(4)?,
                overview: r.get(5)?,
                poster_path: r.get(6)?,
                backdrop_path: r.get(7)?,
                runtime_minutes: r.get(8)?,
                metadata_updated_at: r.get(9)?,
                status: r.get(10)?,
                tagline: r.get(11)?,
                last_air_date: r.get(12)?,
                season_count: r.get(13)?,
                episode_count: r.get(14)?,
                details_fetched_at: r.get(15)?,
            })
        },
    )?;
    let external_refs = rows(
        db,
        "SELECT local_media_id, source, media_type, external_id
        FROM external_refs ORDER BY source, media_type, external_id",
        |r| {
            Ok(ExternalRef {
                media_id: r.get(0)?,
                source: r.get(1)?,
                media_type: r.get(2)?,
                external_id: r.get(3)?,
            })
        },
    )?;
    let library = rows(
        db,
        "SELECT local_media_id, added_at FROM library_entries
        ORDER BY local_media_id",
        |r| {
            Ok(LibraryEntry {
                media_id: r.get(0)?,
                added_at: r.get(1)?,
            })
        },
    )?;
    let genres = rows(
        db,
        "SELECT source, external_id, name FROM genres
        ORDER BY source, external_id",
        |r| {
            Ok(Genre {
                source: r.get(0)?,
                external_id: r.get(1)?,
                name: r.get(2)?,
            })
        },
    )?;
    let media_genres = rows(
        db,
        "SELECT local_media_id, source, external_id FROM media_genres
        ORDER BY local_media_id, source, external_id",
        |r| {
            Ok(MediaGenre {
                media_id: r.get(0)?,
                source: r.get(1)?,
                external_id: r.get(2)?,
            })
        },
    )?;
    let seasons = rows(
        db,
        "SELECT local_media_id, season_number, external_id, name,
        overview, air_date, episode_count, poster_path, episodes_fetched_at,
        episodes_known, metadata_updated_at FROM seasons
        ORDER BY local_media_id, season_number",
        |r| {
            Ok(Season {
                media_id: r.get(0)?,
                number: r.get(1)?,
                external_id: r.get(2)?,
                name: r.get(3)?,
                overview: r.get(4)?,
                air_date: r.get(5)?,
                episode_count: r.get(6)?,
                poster_path: r.get(7)?,
                episodes_fetched_at: r.get(8)?,
                episodes_known: r.get(9)?,
                metadata_updated_at: r.get(10)?,
            })
        },
    )?;
    let episodes = rows(
        db,
        "SELECT local_media_id, season_number, episode_number,
        external_id, name, overview, air_date, runtime_minutes, still_path,
        metadata_updated_at FROM episodes ORDER BY local_media_id, season_number,
        episode_number",
        |r| {
            Ok(Episode {
                media_id: r.get(0)?,
                season: r.get(1)?,
                number: r.get(2)?,
                external_id: r.get(3)?,
                name: r.get(4)?,
                overview: r.get(5)?,
                air_date: r.get(6)?,
                runtime_minutes: r.get(7)?,
                still_path: r.get(8)?,
                metadata_updated_at: r.get(9)?,
            })
        },
    )?;
    let media_tracking = rows(
        db,
        "SELECT local_media_id, media_type, watched_at, rating
        FROM media_tracking ORDER BY local_media_id",
        |r| {
            Ok(MediaTracking {
                media_id: r.get(0)?,
                media_type: r.get(1)?,
                watched_at: r.get(2)?,
                rating: r.get(3)?,
            })
        },
    )?;
    let episode_tracking = rows(
        db,
        "SELECT local_media_id, season_number, episode_number,
        watched_at FROM episode_tracking ORDER BY local_media_id, season_number,
        episode_number",
        |r| {
            Ok(EpisodeTracking {
                media_id: r.get(0)?,
                season: r.get(1)?,
                episode: r.get(2)?,
                watched_at: r.get(3)?,
            })
        },
    )?;
    let watch_events = rows(
        db,
        "SELECT event_id, local_media_id, media_type,
        season_number, episode_number, watched_at, runtime_minutes FROM watch_events
        ORDER BY watched_at, event_id",
        |r| {
            Ok(WatchEvent {
                id: r.get(0)?,
                media_id: r.get(1)?,
                media_type: r.get(2)?,
                season: r.get(3)?,
                episode: r.get(4)?,
                watched_at: r.get(5)?,
                runtime_minutes: r.get(6)?,
            })
        },
    )?;
    let settings = db
        .conn()
        .query_row(
            "SELECT automatic_refresh_enabled FROM app_settings WHERE id = 1",
            [],
            |r| {
                Ok(BackupSettings {
                    automatic_refresh_enabled: r.get(0)?,
                })
            },
        )
        .map_err(read_error)?;
    let release_events = rows(
        db,
        "SELECT event_id, local_media_id, season_number,
        episode_number, event_type, discovered_at, air_date, read_at FROM release_events
        ORDER BY discovered_at, event_id",
        |r| {
            Ok(ReleaseEvent {
                id: r.get(0)?,
                media_id: r.get(1)?,
                season: r.get(2)?,
                episode: r.get(3)?,
                kind: r.get(4)?,
                discovered_at: r.get(5)?,
                air_date: r.get(6)?,
                read_at: r.get(7)?,
            })
        },
    )?;
    Ok(BackupV1 {
        format: FORMAT.into(),
        format_version: VERSION,
        created_at,
        application_version: APP_VERSION.into(),
        data: BackupData {
            media,
            external_refs,
            library,
            genres,
            media_genres,
            seasons,
            episodes,
            media_tracking,
            episode_tracking,
            watch_events,
            settings,
            release_events,
        },
    })
}

fn timestamp(value: i64) -> bool {
    (-62_135_596_800..=253_402_300_799).contains(&value)
}

fn date(value: &Option<String>) -> bool {
    value.as_ref().is_none_or(|v| {
        let bytes = v.as_bytes();
        bytes.len() == 10
            && bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    })
}

fn source(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn identity(source_name: &str, external_id: &str) -> bool {
    !external_id.is_empty()
        && (source_name != "tmdb"
            || (external_id.as_bytes()[0] != b'0'
                && external_id.bytes().all(|b| b.is_ascii_digit())))
}

/// Complete structural and referential validation, before any write.
pub fn validate(backup: &BackupV1) -> Result<(), AppError> {
    if backup.format != FORMAT {
        return Err(invalid("This is not a Bingee backup."));
    }
    if backup.format_version > VERSION {
        return Err(invalid(format!(
            "This backup needs a newer Bingee version (format version {}).",
            backup.format_version
        )));
    }
    if backup.format_version != VERSION {
        return Err(invalid("Unsupported Bingee backup version."));
    }
    if !timestamp(backup.created_at) || backup.application_version.is_empty() {
        return Err(invalid(
            "Backup header has an invalid timestamp or application version.",
        ));
    }
    let data = &backup.data;
    let mut media = HashMap::new();
    for m in &data.media {
        if m.id <= 0
            || !matches!(m.media_type.as_str(), "movie" | "tv")
            || m.title.is_empty()
            || !date(&m.release_date)
            || !date(&m.last_air_date)
            || m.runtime_minutes.is_some_and(|n| n <= 0)
            || m.season_count.is_some_and(|n| n < 0)
            || m.episode_count.is_some_and(|n| n < 0)
            || m.metadata_updated_at.is_some_and(|n| !timestamp(n))
            || m.details_fetched_at.is_some_and(|n| !timestamp(n))
            || media.insert(m.id, m.media_type.as_str()).is_some()
        {
            return Err(invalid("Backup contains invalid or duplicate media."));
        }
    }
    let mut refs = HashSet::new();
    let mut referenced_media = HashSet::new();
    for r in &data.external_refs {
        if !source(&r.source)
            || !identity(&r.source, &r.external_id)
            || media.get(&r.media_id).copied() != Some(r.media_type.as_str())
            || !refs.insert((&r.source, &r.media_type, &r.external_id))
        {
            return Err(invalid(
                "Backup contains an invalid or duplicate provider identity.",
            ));
        }
        referenced_media.insert(r.media_id);
    }
    if media.keys().any(|id| !referenced_media.contains(id)) {
        return Err(invalid("Backup media is missing its provider identity."));
    }
    let mut members = HashSet::new();
    for m in &data.library {
        if !media.contains_key(&m.media_id) || !timestamp(m.added_at) || !members.insert(m.media_id)
        {
            return Err(invalid("Backup has an invalid Library reference."));
        }
    }
    let mut genres = HashSet::new();
    for g in &data.genres {
        if !source(&g.source)
            || g.external_id.is_empty()
            || g.name.is_empty()
            || !genres.insert((&g.source, &g.external_id))
        {
            return Err(invalid("Backup contains an invalid or duplicate genre."));
        }
    }
    let mut links = HashSet::new();
    for g in &data.media_genres {
        if !media.contains_key(&g.media_id)
            || !genres.contains(&(&g.source, &g.external_id))
            || !links.insert((g.media_id, &g.source, &g.external_id))
        {
            return Err(invalid("Backup has an invalid genre relationship."));
        }
    }
    let mut seasons = HashSet::new();
    for s in &data.seasons {
        if media.get(&s.media_id).copied() != Some("tv")
            || s.number < 0
            || !date(&s.air_date)
            || s.episode_count.is_some_and(|n| n < 0)
            || s.episodes_known.is_some_and(|n| n < 0)
            || s.episodes_fetched_at.is_some_and(|n| !timestamp(n))
            || s.metadata_updated_at.is_some_and(|n| !timestamp(n))
            || !seasons.insert((s.media_id, s.number))
        {
            return Err(invalid("Backup contains an invalid or duplicate season."));
        }
    }
    let mut episodes = HashSet::new();
    let mut episode_ids = HashSet::new();
    for e in &data.episodes {
        if !seasons.contains(&(e.media_id, e.season))
            || e.number < 0
            || !date(&e.air_date)
            || e.runtime_minutes.is_some_and(|n| n <= 0)
            || e.metadata_updated_at.is_some_and(|n| !timestamp(n))
            || !episodes.insert((e.media_id, e.season, e.number))
            || e.external_id
                .as_ref()
                .is_some_and(|id| id.is_empty() || !episode_ids.insert((e.media_id, e.season, id)))
        {
            return Err(invalid("Backup contains an invalid or duplicate episode."));
        }
    }
    let mut title_tracking = HashSet::new();
    for t in &data.media_tracking {
        if media.get(&t.media_id).copied() != Some(t.media_type.as_str())
            || t.watched_at.is_some_and(|n| !timestamp(n))
            || t.rating.is_some_and(|n| !(1..=10).contains(&n))
            || !title_tracking.insert(t.media_id)
        {
            return Err(invalid(
                "Backup has an invalid title tracking reference or rating.",
            ));
        }
    }
    let mut episode_tracking = HashSet::new();
    for t in &data.episode_tracking {
        // Provider metadata may have removed an episode. Its personal state
        // must still survive, so only the series and numeric key are required.
        if media.get(&t.media_id).copied() != Some("tv")
            || t.season < 0
            || t.episode < 0
            || !timestamp(t.watched_at)
            || !episode_tracking.insert((t.media_id, t.season, t.episode))
        {
            return Err(invalid("Backup has an invalid episode tracking reference."));
        }
    }
    let mut events = HashSet::new();
    for e in &data.watch_events {
        let target = match (e.media_type.as_str(), e.season, e.episode) {
            ("movie", None, None) => true,
            ("tv", Some(s), Some(n)) => s >= 0 && n >= 0,
            _ => false,
        };
        if e.id <= 0
            || !target
            || media.get(&e.media_id).copied() != Some(e.media_type.as_str())
            || !timestamp(e.watched_at)
            || e.runtime_minutes.is_some_and(|n| n <= 0)
            || !events.insert(e.id)
        {
            return Err(invalid(
                "Backup has an invalid watch-history target or event.",
            ));
        }
    }
    let mut releases = HashSet::new();
    let mut release_ids = HashSet::new();
    for e in &data.release_events {
        if e.id <= 0
            || media.get(&e.media_id).copied() != Some("tv")
            || e.season < 0
            || e.episode < 0
            || e.kind != "new_episode"
            || !timestamp(e.discovered_at)
            || e.read_at.is_some_and(|n| !timestamp(n))
            || !date(&e.air_date)
            || !releases.insert((e.media_id, e.season, e.episode, &e.kind))
            || !release_ids.insert(e.id)
        {
            return Err(invalid("Backup has an invalid or duplicate release event."));
        }
    }
    Ok(())
}

pub fn parse(bytes: &[u8]) -> Result<BackupV1, AppError> {
    let backup: BackupV1 = serde_json::from_slice(bytes)
        .map_err(|e| invalid("Backup JSON is malformed or incomplete.").with_source(e))?;
    validate(&backup)?;
    Ok(backup)
}

/// Replace all durable rows in one transaction. Every error before commit
/// rolls back, including a failed constraint or verification check.
pub fn restore(db: &Database, backup: &BackupV1) -> Result<(), AppError> {
    validate(backup)?;
    let d = &backup.data;
    let tx = Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate)
        .map_err(write_error)?;
    tx.execute_batch(
        "DELETE FROM release_events; DELETE FROM watch_events; DELETE FROM episode_tracking;
        DELETE FROM media_tracking; DELETE FROM library_entries;
        DELETE FROM media_genres; DELETE FROM episodes; DELETE FROM seasons;
        DELETE FROM external_refs; DELETE FROM genres; DELETE FROM media;",
    )
    .map_err(write_error)?;
    for m in &d.media {
        tx.execute(
            "INSERT INTO media (local_media_id, media_type, title, original_title,
            release_date, overview, poster_path, backdrop_path, runtime_minutes,
            metadata_updated_at, status, tagline, last_air_date, season_count,
            episode_count, details_fetched_at) VALUES
            (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                m.id,
                m.media_type,
                m.title,
                m.original_title,
                m.release_date,
                m.overview,
                m.poster_path,
                m.backdrop_path,
                m.runtime_minutes,
                m.metadata_updated_at,
                m.status,
                m.tagline,
                m.last_air_date,
                m.season_count,
                m.episode_count,
                m.details_fetched_at
            ],
        )
        .map_err(write_error)?;
    }
    for r in &d.external_refs {
        tx.execute(
            "INSERT INTO external_refs (local_media_id, source, media_type, external_id)
            VALUES (?1, ?2, ?3, ?4)",
            params![r.media_id, r.source, r.media_type, r.external_id],
        )
        .map_err(write_error)?;
    }
    for l in &d.library {
        tx.execute(
            "INSERT INTO library_entries (local_media_id, added_at) VALUES (?1, ?2)",
            params![l.media_id, l.added_at],
        )
        .map_err(write_error)?;
    }
    for g in &d.genres {
        tx.execute(
            "INSERT INTO genres (source, external_id, name) VALUES (?1, ?2, ?3)",
            params![g.source, g.external_id, g.name],
        )
        .map_err(write_error)?;
    }
    for g in &d.media_genres {
        tx.execute(
            "INSERT INTO media_genres (local_media_id, source, external_id)
            VALUES (?1, ?2, ?3)",
            params![g.media_id, g.source, g.external_id],
        )
        .map_err(write_error)?;
    }
    for s in &d.seasons {
        tx.execute(
            "INSERT INTO seasons (local_media_id, media_type, season_number,
            external_id, name, overview, air_date, episode_count, poster_path,
            episodes_fetched_at, episodes_known, metadata_updated_at)
            VALUES (?1, 'tv', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                s.media_id,
                s.number,
                s.external_id,
                s.name,
                s.overview,
                s.air_date,
                s.episode_count,
                s.poster_path,
                s.episodes_fetched_at,
                s.episodes_known,
                s.metadata_updated_at
            ],
        )
        .map_err(write_error)?;
    }
    for e in &d.episodes {
        tx.execute(
            "INSERT INTO episodes (local_media_id, season_number, episode_number,
            external_id, name, overview, air_date, runtime_minutes, still_path,
            metadata_updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                e.media_id,
                e.season,
                e.number,
                e.external_id,
                e.name,
                e.overview,
                e.air_date,
                e.runtime_minutes,
                e.still_path,
                e.metadata_updated_at
            ],
        )
        .map_err(write_error)?;
    }
    for t in &d.media_tracking {
        tx.execute(
            "INSERT INTO media_tracking (local_media_id, media_type, watched_at, rating)
            VALUES (?1, ?2, ?3, ?4)",
            params![t.media_id, t.media_type, t.watched_at, t.rating],
        )
        .map_err(write_error)?;
    }
    for t in &d.episode_tracking {
        tx.execute(
            "INSERT INTO episode_tracking (local_media_id, media_type, season_number,
            episode_number, watched_at) VALUES (?1, 'tv', ?2, ?3, ?4)",
            params![t.media_id, t.season, t.episode, t.watched_at],
        )
        .map_err(write_error)?;
    }
    for e in &d.watch_events {
        tx.execute(
            "INSERT INTO watch_events (event_id, local_media_id, media_type,
            season_number, episode_number, watched_at, runtime_minutes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                e.id,
                e.media_id,
                e.media_type,
                e.season,
                e.episode,
                e.watched_at,
                e.runtime_minutes
            ],
        )
        .map_err(write_error)?;
    }
    tx.execute(
        "UPDATE app_settings SET automatic_refresh_enabled = ?1 WHERE id = 1",
        [d.settings.automatic_refresh_enabled],
    )
    .map_err(write_error)?;
    for e in &d.release_events {
        tx.execute(
            "INSERT INTO release_events (event_id, local_media_id, media_type,
            season_number, episode_number, event_type, discovered_at, air_date, read_at)
            VALUES (?1, ?2, 'tv', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                e.id,
                e.media_id,
                e.season,
                e.episode,
                e.kind,
                e.discovered_at,
                e.air_date,
                e.read_at
            ],
        )
        .map_err(write_error)?;
    }
    let foreign_keys: i64 = tx
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .map_err(write_error)?;
    let integrity: String = tx
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(write_error)?;
    if foreign_keys != 0 || integrity != "ok" {
        return Err(invalid(
            "Restored database failed integrity checks. Original data is unchanged.",
        ));
    }
    for (table, expected) in [
        ("media", d.media.len()),
        ("external_refs", d.external_refs.len()),
        ("library_entries", d.library.len()),
        ("genres", d.genres.len()),
        ("media_genres", d.media_genres.len()),
        ("seasons", d.seasons.len()),
        ("episodes", d.episodes.len()),
        ("media_tracking", d.media_tracking.len()),
        ("episode_tracking", d.episode_tracking.len()),
        ("watch_events", d.watch_events.len()),
        ("release_events", d.release_events.len()),
    ] {
        let actual: i64 = tx
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(write_error)?;
        if actual != expected as i64 {
            return Err(invalid(
                "Restored database row counts differ. Original data is unchanged.",
            ));
        }
    }
    tx.commit().map_err(write_error)
}

/// Create a new backup file without truncating an existing one. A same-folder
/// hard link publishes the fully written file without replacing another file.
pub fn export_to_path(db: &Database, path: &Path, now: i64) -> Result<(), AppError> {
    if path.exists() {
        return Err(invalid(
            "A file already exists at that backup path. Choose a new name.",
        ));
    }
    let backup = export(db, now)?;
    let bytes = serde_json::to_vec_pretty(&backup)
        .map_err(|e| invalid("Backup could not be serialized.").with_source(e))?;
    let mut temp = path.as_os_str().to_os_string();
    temp.push(format!(".{}.tmp", std::process::id()));
    let temp = Path::new(&temp);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp)
            .map_err(|e| AppError::filesystem("Backup destination is not writable.", e))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| AppError::filesystem("Backup file could not be written.", e))?;
        // Hard-link creation fails if the destination appeared after our
        // existence check; rename would replace that file on Unix.
        fs::hard_link(temp, path).map_err(|e| {
            AppError::filesystem("Backup file could not be finalized on this filesystem.", e)
        })?;
        let _ = fs::remove_file(temp);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

/// Before a user-confirmed replacement, retain the current portable state
/// beside the live database. In-memory test databases have no file location.
pub fn pre_restore_safety(db: &Database, now: i64) -> Result<Option<PathBuf>, AppError> {
    let file: String = db
        .conn()
        .query_row("PRAGMA database_list", [], |r| r.get(2))
        .map_err(read_error)?;
    if file.is_empty() {
        return Ok(None);
    }
    let parent = Path::new(&file)
        .parent()
        .ok_or_else(|| invalid("Database path has no folder."))?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| invalid("System clock could not name the safety backup.").with_source(e))?
        .as_nanos();
    let path = parent.join(format!("bingee-pre-restore-{now}-{nanos}.json"));
    export_to_path(db, &path, now)?;
    Ok(Some(path))
}

pub fn read_from_path(path: &Path) -> Result<BackupV1, AppError> {
    let length = fs::metadata(path)
        .map_err(|e| AppError::filesystem("Backup file could not be read.", e))?
        .len();
    if length > 256 * 1024 * 1024 {
        return Err(invalid("Backup file exceeds the 256 MiB limit."));
    }
    let bytes =
        fs::read(path).map_err(|e| AppError::filesystem("Backup file could not be read.", e))?;
    parse(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{self, MediaType};
    use crate::metadata::tests::{episode, genre, movie, season, series, stored};
    use crate::metadata::{save, save_episodes};
    use crate::paths::TestDir;
    use crate::statistics::{self, Range};
    use crate::tracking;

    const NOW: i64 = 1_800_000_000;

    fn populated(db: &Database) {
        let film = stored(db, MediaType::Movie, 603, "Film");
        let show = stored(db, MediaType::Tv, 603, "Series");
        save(db, film, &movie("Film", vec![genre("28", "Action")]), 100).unwrap();
        save(
            db,
            show,
            &series("Series", vec![season(0, Some(1)), season(1, Some(3))]),
            200,
        )
        .unwrap();
        save_episodes(db, show, 0, &[episode(0, 1)], 300).unwrap();
        save_episodes(db, show, 1, &[episode(1, 1), episode(1, 2)], 300).unwrap();
        tracking::watch_movie(db, film, NOW - 10, false).unwrap();
        tracking::set_rating(db, film, Some(9)).unwrap();
        tracking::watch_episode(db, show, 0, 1, NOW - 9, false).unwrap();
        tracking::watch_episode(db, show, 1, 1, NOW - 8, false).unwrap();
        tracking::watch_episode(db, show, 1, 1, NOW - 7, true).unwrap();
        library::remove(db, film).unwrap();
        crate::settings::set_automatic_refresh(db, true).unwrap();
        db.conn()
            .execute(
                "INSERT INTO release_events (local_media_id, media_type,
            season_number, episode_number, event_type, discovered_at, air_date, read_at)
            VALUES (?1, 'tv', 1, 2, 'new_episode', ?2, '2026-09-30', ?3)",
                rusqlite::params![show, NOW - 6, NOW - 5],
            )
            .unwrap();
    }

    #[test]
    fn empty_and_populated_round_trip_preserve_semantics() {
        let empty = Database::open_in_memory();
        let empty_backup = export(&empty, NOW).unwrap();
        let target = Database::open_in_memory();
        restore(
            &target,
            &parse(&serde_json::to_vec(&empty_backup).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(export(&target, NOW).unwrap(), empty_backup);

        populated(&empty);
        let before = export(&empty, NOW).unwrap();
        validate(&before).unwrap();
        assert_eq!(before.data.media.len(), 2);
        assert_eq!(
            before
                .data
                .external_refs
                .iter()
                .filter(|r| r.external_id == "603")
                .count(),
            2
        );
        assert_eq!(
            before.data.library.len(),
            1,
            "removed movie stays in backup metadata"
        );
        assert_eq!(before.data.watch_events.len(), 4);
        assert_eq!(before.data.watch_events[0].runtime_minutes, Some(136));
        assert!(before.data.settings.automatic_refresh_enabled);
        assert_eq!(before.data.release_events.len(), 1);
        assert_eq!(before.data.release_events[0].read_at, Some(NOW - 5));
        let stats = statistics::load(&empty, Range::AllTime, NOW).unwrap();
        restore(
            &target,
            &parse(&serde_json::to_vec_pretty(&before).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(export(&target, NOW).unwrap(), before);
        assert_eq!(
            statistics::load(&target, Range::AllTime, NOW).unwrap(),
            stats
        );
        assert_eq!(
            tracking::recent_history(&target, 10).unwrap(),
            tracking::recent_history(&empty, 10).unwrap()
        );
        assert!(
            target
                .conn()
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap()
                == 0
        );
    }

    #[test]
    fn movie_only_tv_only_and_partial_coverage_round_trip() {
        for kind in [MediaType::Movie, MediaType::Tv] {
            let source = Database::open_in_memory();
            let id = stored(&source, kind, 603, "One title");
            if kind == MediaType::Tv {
                save(
                    &source,
                    id,
                    &series("One title", vec![season(0, Some(1)), season(1, Some(3))]),
                    1,
                )
                .unwrap();
                save_episodes(&source, id, 1, &[episode(1, 1), episode(1, 2)], 2).unwrap();
            }
            let before = export(&source, NOW).unwrap();
            let target = Database::open_in_memory();
            restore(
                &target,
                &parse(&serde_json::to_vec(&before).unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(export(&target, NOW).unwrap(), before);
            if kind == MediaType::Tv {
                let seasons = crate::metadata::read(&target, id)
                    .unwrap()
                    .unwrap()
                    .tv
                    .unwrap()
                    .seasons;
                assert_eq!(seasons[0].coverage(), crate::metadata::Coverage::NotFetched);
                assert_eq!(
                    seasons[1].coverage(),
                    crate::metadata::Coverage::Partial {
                        known: 2,
                        expected: 3
                    }
                );
                assert_eq!(seasons[1].episodes_fetched_at, Some(2));
            }
        }
    }

    #[test]
    fn invalid_inputs_never_change_original() {
        let db = Database::open_in_memory();
        populated(&db);
        let before = export(&db, NOW).unwrap();
        for bytes in [b"{".as_slice(), b"[]".as_slice(), b"".as_slice()] {
            assert!(parse(bytes).is_err());
        }
        for change in 0..11 {
            let mut broken = before.clone();
            match change {
                0 => broken.format = "other".into(),
                1 => broken.format_version = 2,
                2 => broken.data.media[0].media_type = "unknown".into(),
                3 => broken
                    .data
                    .external_refs
                    .push(broken.data.external_refs[0].clone()),
                4 => broken.data.media_tracking[0].rating = Some(11),
                5 => broken.data.episode_tracking[0].media_id = 99999,
                6 => broken.data.watch_events[0].media_id = 99999,
                7 => broken.data.episodes[0].season = 99999,
                8 => broken.data.genres[0].name.clear(),
                9 => broken.data.release_events[0].media_id = 99999,
                _ => broken
                    .data
                    .release_events
                    .push(broken.data.release_events[0].clone()),
            }
            assert!(restore(&db, &broken).is_err(), "case {change}");
            assert_eq!(export(&db, NOW).unwrap(), before);
        }
    }

    #[test]
    fn import_failure_rolls_back_every_row() {
        let source = Database::open_in_memory();
        populated(&source);
        let backup = export(&source, NOW).unwrap();
        let live = Database::open_in_memory();
        stored(&live, MediaType::Movie, 99, "Keep me");
        let before = export(&live, NOW).unwrap();
        live.conn()
            .execute_batch(
                "CREATE TEMP TRIGGER reject_episode BEFORE INSERT ON episodes
            BEGIN SELECT RAISE(ABORT, 'forced import failure'); END;",
            )
            .unwrap();
        assert!(restore(&live, &backup).is_err());
        assert_eq!(export(&live, NOW).unwrap(), before);
    }

    #[test]
    fn file_export_is_atomic_and_does_not_overwrite() {
        let dir = TestDir::new("backup-files");
        let db = Database::open_in_memory();
        populated(&db);
        let path = dir.0.join("backup.json");
        export_to_path(&db, &path, NOW).unwrap();
        let original = fs::read(&path).unwrap();
        assert!(export_to_path(&db, &path, NOW).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        let target = Database::open_in_memory();
        restore(&target, &read_from_path(&path).unwrap()).unwrap();
        assert_eq!(export(&target, NOW).unwrap(), export(&db, NOW).unwrap());
        assert!(export_to_path(&db, &dir.0.join("missing").join("no.json"), NOW).is_err());
        assert!(read_from_path(&dir.0.join("missing.json")).is_err());
    }

    #[test]
    fn pre_restore_safety_copy_keeps_previous_portable_state() {
        let dir = TestDir::new("pre-restore-safety");
        let db = Database::open(
            &dir.0.join("bingee.db"),
            &crate::diagnostics::Log::stderr_only(),
        )
        .unwrap();
        populated(&db);
        let before = export(&db, NOW).unwrap();
        let path = pre_restore_safety(&db, NOW).unwrap().unwrap();
        assert_eq!(path.parent(), Some(dir.0.as_path()));
        assert_eq!(read_from_path(&path).unwrap(), before);
        assert!(
            pre_restore_safety(&Database::open_in_memory(), NOW)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    #[ignore]
    fn informal_large_backup_timings() {
        use crate::statistics::tests::large_history;
        use std::time::Instant;
        let db = Database::open_in_memory();
        large_history(&db);
        let started = Instant::now();
        let backup = export(&db, NOW).unwrap();
        println!("export {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
        let started = Instant::now();
        let bytes = serde_json::to_vec(&backup).unwrap();
        println!(
            "serialize {:.1} ms, {} bytes",
            started.elapsed().as_secs_f64() * 1e3,
            bytes.len()
        );
        let started = Instant::now();
        let parsed = parse(&bytes).unwrap();
        println!(
            "parse + validate {:.1} ms",
            started.elapsed().as_secs_f64() * 1e3
        );
        let target = Database::open_in_memory();
        let started = Instant::now();
        restore(&target, &parsed).unwrap();
        println!("restore {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
        assert_eq!(export(&target, NOW).unwrap(), backup);
    }
}
