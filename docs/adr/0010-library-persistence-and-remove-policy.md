# ADR-0010: Library persistence: atomic idempotent add, non-destructive remove

- Status: Proposed
- Date: 2026-09-14

## Context

R8 turns transient TMDB search results (R7) into library data. Schema v1
(ADR-0006) already separates cached metadata (`media`), provider identity
(`external_refs`, key `(source, media_type, external_id)`) and membership
(`library_entries`). Future watch history will hang off `media`, not off
membership. The library must work from SQLite alone, offline, without a
token.

## Decision

- **No schema change.** v1 already has every field R8 persists (title,
  original title, type, ISO release date, overview, poster and backdrop
  paths, `metadata_updated_at`). No v2 migration is created just to have one.
- **Add** (`library::add`) is one `BEGIN IMMEDIATE` transaction:
  1. find `media` through `external_refs (source, media_type, external_id)`;
  2. found: refresh the search-provided metadata, keeping stored values where
     the search result has none (`COALESCE`), never touching
     `runtime_minutes` (not in search results); not found: insert `media`,
     then the `external_refs` row;
  3. insert `library_entries (local_media_id, added_at)` unless it exists.
  Any failure rolls back everything. `metadata_updated_at` and `added_at`
  are the current Unix time in seconds (UTC), as v1 documents.
- **Idempotent**: adding a title that is already in the library changes no
  membership and keeps its original `added_at`; the result reports "already
  in library". Identity is only ever the SQLite key; the app never matches
  titles by name or number.
- **Remove** (`library::remove`) deletes the `library_entries` row only.
  `media`, `external_refs`, and cached posters stay, so a later re-add reuses
  them (new `added_at`, same `local_media_id`) and future watch history can
  outlive a membership change.
- **Reads**: the Library page queries `library_entries ⋈ media` only: search
  (Unicode case folding on title and original title), a Movie/TV filter, and
  sorting by recently added (`added_at DESC, local_media_id DESC`) or title.
  No index beyond v1's: a personal library sorts in memory in SQLite well
  within a frame (see R8 measurements).
- **Membership for Discover**: one query per result set, joining the
  identities passed as a JSON array (`json_each`) to `external_refs` and
  `library_entries`. No per-row query.
- **Writes happen on the UI thread**, like every R6 library query: one small
  local transaction, measured in milliseconds.

## Alternatives considered

- **Delete `media`/`external_refs` on remove**: tidy, but destroys data that
  R9 watch history will reference and forces re-downloading metadata.
  Cleanup of orphaned metadata needs an explicit policy later.
- **Replace metadata on add**: simpler SQL, but a search result without an
  overview would erase one fetched earlier.
- **Bump `added_at` on a repeated add**: turns an idempotent action into a
  reordering side effect.
- **One membership query per visible result**: N+1 queries on every render.

## Consequences

### Positive

- The library is complete and usable offline; TMDB is only needed to find
  and add titles.
- Re-adding is cheap and never duplicates metadata.

### Negative / risks

- Orphaned `media` rows and posters accumulate after removals until a
  cleanup policy exists (R9 or later).
- `added_at` has one-second resolution; titles added in the same second sort
  by local id.

## Validation

Database tests: Movie add, TV add, repeated add, Movie/TV with the same TMDB
id, forced failures after the `media` insert, at the `external_refs` insert
and at the membership insert (temporary triggers) leave no partial rows,
remove keeps metadata, re-add reuses it, data survives reopening, membership
lookup for mixed identities.

## Revisit trigger

Watch history (R9+), a metadata refresh policy, disk-space complaints about
orphaned data, or library sizes where sorting becomes measurably slow.
