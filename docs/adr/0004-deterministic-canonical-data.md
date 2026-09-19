# ADR 0004: Deterministic canonical data; AI and discovery are advisory

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh may use system discovery and AI to infer relationships or enrich metadata. Incorrect automatic changes would damage trust in a personal inventory.

## Decision

Provider/discovery/AI output is treated as a candidate or suggestion until accepted by deterministic rules or explicit user confirmation.

Canonical asset data must record its source where relevant.

## Consequences

Positive:

- trustworthy inventory;
- explainable imports and discovery;
- easier rollback/review.

Costs:

- more confirmation UX;
- less “magical” automation in early versions.
