# ADR 0001: Local-first and UI-independent core

- Status: Accepted
- Date: 2026-09-19

## Context

AssetMesh may eventually have a desktop app, CLI, optional web access, and agent integrations. Binding business logic to one presentation technology would make reuse difficult.

## Decision

Canonical data is local-first. Domain and application logic live in a headless core independent of React, Tauri, HTTP, and concrete database APIs.

Presentation surfaces call the same application layer through adapters.

## Consequences

Positive:

- desktop/CLI reuse;
- optional future HTTP/MCP adapters;
- easier testing;
- no mandatory localhost server for desktop use.

Costs:

- requires explicit adapter boundaries;
- slightly more upfront structure than putting SQL directly in UI commands.
