# ADR-0027: Local release events and in-app notifications

- Status: Accepted
- Date: 2026-09-29

## Context

Automatic metadata refresh may discover episodes a user has not seen. Events
must survive restart and repeated refresh without changing watched state.

## Decision

Schema v5 adds `release_events` for a new episode discovered in a Library
season whose episode list was downloaded before. The key is local series,
season number, episode number, and event kind (`new_episode`). One unique
constraint prevents duplicates after rediscovery. Event insertion runs in
the same transaction as the episode metadata save. `read_at` is nullable and
is never updated by provider refresh. Notification Center lists recent
events and supports individual or bulk Mark Read. Calendar and Home reload
after a successful metadata update. Native OS notifications are deferred.

## Alternatives considered

- Generating events for a season's first download would flood the center
  with already known back-catalogue episodes.
- Deriving notifications from current episode rows loses discovery time,
  read state, and deduplication across restarts.
- OS notifications first would add platform work before local event semantics
  are reliable.

## Consequences

An episode whose number changes can produce a new event; the provider has
changed its identity under the current numeric tracking model. Removed
Library titles retain existing events but receive no new ones.

## Validation

Tests cover first download, new episode, repeated refresh, restart, read
state, Library scope, and watched-state firewall.

## Revisit trigger

Provider renumbering creates frequent misleading events or users need air
date changes as a distinct actionable event.
