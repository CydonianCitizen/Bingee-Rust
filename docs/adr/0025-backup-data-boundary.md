# ADR-0025: Backup inclusion and exclusion boundary

- Status: Accepted
- Date: 2026-09-29

## Context

User data must survive a restore while credentials and replaceable files must
remain outside the portable archive.

## Decision

V1 includes provider metadata needed for an offline Library, all external
identities, membership, genres, season and episode metadata, separate
freshness and coverage fields, movie and episode tracking, ratings, and each
watch event with its runtime snapshot. Removed Library titles remain when
their metadata or personal state remains in SQLite. Provider image paths are
metadata and remain included.

V1 excludes TMDB credentials in the OS credential store, decoded or disk
image caches, logs, temporary files, benchmark data, and diagnostics. Export
reads only the named durable tables. It never reads the credential store.

## Alternatives considered

- A file-system archive of the data directory might include logs and caches
  and omit OS credentials unpredictably.
- Exporting only current Library membership would lose watch history and
  ratings of removed titles.

## Consequences

Posters may need downloading after restore, but offline text metadata and all
personal state remain available. Backup size grows with metadata and history.

## Validation

Round-trip tests compare all V1 arrays and Statistics. Source inspection and
file tests verify the export reads only explicit database tables.

## Revisit trigger

A new durable personal table is added. The backup contract must be versioned
or extended with a backward-compatible optional field before shipping it.
