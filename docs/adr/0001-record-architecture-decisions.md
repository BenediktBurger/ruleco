# 1. Record Architecture Decisions

Date: 2026-05-23

## Status

Accepted

## Context

RuLECO is a workspace with several semi-independent crates. The Coordinator uses definitions from core, but communication between programs is determined by the LECO protocol, so there is no coupling between different programs except that they share core for convenience.

We need a way to record architectural decisions so they are:
- Discoverable by anyone working on the project
- Traceable (why was a decision made, what alternatives were considered)
- Scoped (some decisions are workspace-wide, others are crate-specific)

## Decision

We will record architecture decisions as Architecture Decision Records (ADRs) in `docs/adr/`.

### Structure

- Workspace-wide ADRs go directly in `docs/adr/` (e.g. decisions about inter-crate boundaries, shared conventions, protocol choices)
- Crate-specific ADRs go in `docs/adr/<crate-name>/` (e.g. decisions about internal architecture of a single crate)
- Subdirectories are created on demand — only when a crate has its first ADR to record

### Format

Each ADR file follows the Michael Nygard format:
- **Title** (in heading and filename)
- **Date**
- **Status** (Proposed / Accepted / Deprecated / Superseded)
- **Context** — why the decision is needed
- **Decision** — what was decided
- **Consequences** — what results from the decision

### Naming

Files are numbered sequentially per directory: `0001-short-title.md`, `0002-another-decision.md`. Numbering is independent per scope (workspace-level and each crate subdirectory).

## Consequences

- Architectural decisions are documented and findable in one location under `docs/`
- The scope of each ADR is clear from its location (top-level vs. crate subdirectory)
- We avoid premature subdirectories — they appear only when needed
- New crates (e.g. future programs that talk to the Coordinator) will get their own subdirectory when they have their first ADR
