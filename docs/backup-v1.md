# Bingee backup format V1

Portable file: UTF-8 JSON. Top-level object has exactly the required
concepts `format` (`"bingee-backup"`), `format_version` (`1`), `created_at`
(Unix seconds UTC), `application_version` (string), and `data` (object).
Unknown newer format versions are rejected. Missing required fields fail
parsing. `data` has these arrays:

| Array | Fields |
| --- | --- |
| `media` | `id`, `media_type`, `title`, `original_title`, `release_date`, `overview`, `poster_path`, `backdrop_path`, `runtime_minutes`, `metadata_updated_at`, `status`, `tagline`, `last_air_date`, `season_count`, `episode_count`, `details_fetched_at` |
| `external_refs` | `media_id`, `source`, `media_type`, `external_id` |
| `library` | `media_id`, `added_at` |
| `genres` | `source`, `external_id`, `name` |
| `media_genres` | `media_id`, `source`, `external_id` |
| `seasons` | `media_id`, `number`, `external_id`, `name`, `overview`, `air_date`, `episode_count`, `poster_path`, `episodes_fetched_at`, `episodes_known`, `metadata_updated_at` |
| `episodes` | `media_id`, `season`, `number`, `external_id`, `name`, `overview`, `air_date`, `runtime_minutes`, `still_path`, `metadata_updated_at` |
| `media_tracking` | `media_id`, `media_type`, `watched_at`, `rating` |
| `episode_tracking` | `media_id`, `season`, `episode`, `watched_at` |
| `watch_events` | `id`, `media_id`, `media_type`, `season`, `episode`, `watched_at`, `runtime_minutes` |
| `release_events` | `id`, `media_id`, `season`, `episode`, `kind` (`new_episode`), `discovered_at`, `air_date`, `read_at` |

`data.settings` is an object with `automatic_refresh_enabled` (boolean).
Both `settings` and `release_events` default to off/empty when absent, so
V1 files exported before R14 remain importable. New exporters include them.

`id` and `media_id` are identifiers local to this backup. `watch_events.id`
preserves event order when timestamps match. Future importers may remap both
kinds of identifier. Provider identity is always the triple (`source`,
`media_type`, `external_id`); Movie and TV with equal TMDB numeric IDs remain
distinct. `media_type` is `movie` or `tv`. `source` starts with a lowercase
ASCII letter and otherwise uses lowercase letters, digits, or underscore.
For TMDB, `external_id` is a positive decimal integer without leading zero.

Optional fields are JSON `null` when unknown. Dates are `YYYY-MM-DD` provider
dates; they are never converted through a time zone. Timestamp fields are
Unix seconds UTC. `rating` is an integer 1–10 or null. Season 0 is a valid
specials season. An episode tracking or history target may refer to an episode
that provider metadata no longer lists; import preserves that target.
`runtime_minutes` on each watch event is the historical snapshot and is never
recomputed from current metadata during V1 restore.

Arrays are exported in stable key order: media by backup ID; identities and
genres by provider key; membership and tracking by media/episode key; watch
events by timestamp and event ID. Only `created_at` normally changes between
exports of unchanged data.
Release events are ordered by discovery timestamp and event ID. Read state
is personal data and provider refresh never resets it.

Restore validates the entire structure and references before mutation. It
replaces Bingee's durable rows in one SQLite transaction, verifies foreign
keys, integrity, and row counts, then commits. Any pre-commit failure rolls
back. No TMDB request occurs during export or restore.

Credentials, cache images, logs, temporary files, benchmark fixtures, and
diagnostics are excluded. Poster and still paths are included as metadata.

The suggested filename is `bingee-backup.json`; the extension is not trusted
for validation. Export writes and syncs a same-folder temporary file first.
Where supported, a hard link publishes the complete file atomically without
overwriting an existing destination. On other filesystems, Bingee opens the
destination exclusively, copies and syncs it. If the process dies during the
fallback copy, a truncated file may remain at the selected name; parsing and
full V1 validation reject it. A normal copy error removes the incomplete file.
Temporary files have a `.bingee-<pid>-<nanoseconds>.tmp` suffix. Bingee does
not scan arbitrary backup folders to delete old files. A stale temporary file
can be removed manually after confirming no export is running.
