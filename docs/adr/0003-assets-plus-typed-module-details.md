# ADR 0003: Shared Asset identity with typed module details

- Status: Accepted
- Date: 2026-09-19

## Context

Media, software, services, and projects need shared capabilities such as search, tags, collections, relations, and activity. Their domain fields differ substantially.

## Decision

Use a small shared Asset identity and store domain-specific attributes in module-owned typed details.

Avoid a giant universal asset table and avoid completely separate top-level identities for every module.

## Consequences

Positive:

- shared cross-domain capabilities;
- clean module-specific schemas;
- relations can connect all asset kinds.

Costs:

- reading a full asset may require joining/loading typed detail data;
- module ownership and migration boundaries need discipline.
