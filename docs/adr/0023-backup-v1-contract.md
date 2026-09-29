# ADR-0023: Portable backup V1 contract and scope

- Status: Accepted
- Date: 2026-09-29

## Context

Personal watches, ratings, history, and Library membership must survive
reinstallation without TMDB. SQLite file copying would bind backups to one
schema and include implementation details.

## Decision

Use UTF-8 JSON with `format = "bingee-backup"`, `format_version = 1`, Unix
`created_at`, `application_version`, and typed `data`. Data contains media,
external identities, Library membership, genres and links, seasons, episodes,
tracking, and watch events with their runtime snapshots. Backup-local media
and event IDs link records and preserve history order; import may map these
to other database IDs in a later implementation. Arrays have stable ordering.

Credentials, cache images, logs, fixtures, and diagnostic output are excluded.
Poster and still paths are metadata and remain included. V1 import rejects
unknown future versions. Each external identity includes source, media type,
and external ID.

## Alternatives considered

- Raw SQLite backup ties the external contract to the internal schema and
  can include data outside the defined scope.
- A ZIP archive adds a format layer and dependency without benefit for V1.

## Consequences

JSON is inspectable and larger than a compressed archive. Future versions
must keep V1 import support. An event with removed episode metadata retains
its numeric episode target and runtime snapshot.

## Validation

Round trips cover identity collisions, removed Library titles, rewatches,
genres, coverage, freshness, and Statistics. Malformed and future inputs
fail validation before any database mutation.

## Revisit trigger

Measured backup size or processing time becomes unsuitable for desktop use.
