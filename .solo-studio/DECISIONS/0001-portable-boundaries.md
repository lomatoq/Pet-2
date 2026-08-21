# ADR-0001 — Portable state and thin host adapters

Status: accepted
Date: 2026-08-21

## Decision

`lifecore`, `pet_body`, and `pet_audio` contain portable deterministic domain and
presentation data. `desktop_host` alone owns native APIs, physical desktop geometry,
OS data directories, and overlay policy. The app composes these layers through one
`create_platform_backend()` factory.

The serialized state contains a schema version, domain state, procedural definitions,
and normalized monitor position. It excludes native handles, process identifiers,
physical pixels, absolute paths, and device handles.

## Consequences

- Windows and macOS builds share one organism and one fixture.
- Missing sensors are represented by `None` plus a false capability.
- Platform validation can evolve without changing LifeCore.
- LifeCore v0.1 can evolve behind the same snapshot and observation seams without
  introducing platform types into organism state.
