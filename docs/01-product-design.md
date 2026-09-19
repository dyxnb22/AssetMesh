# Product Design

## Information architecture

The product should be organized around assets, not technical subsystems.

```text
Overview

Library
  All Assets
  Media
  Software
  Services
  Knowledge
  Projects

Collections
Relations
Activity

Import / Export
Settings
```

System-oriented views such as processes, containers, health checks, and logs may appear later as contextual views inside relevant asset types rather than as the primary navigation model.

## Primary object: Asset

An Asset is a durable item the user wants AssetMesh to remember.

Examples:

| Category | Examples |
|---|---|
| Media | movie, anime, TV series, game, book |
| Software | macOS app, CLI tool, plugin, agent |
| Service | API, SaaS, VPS, domain, local service |
| Knowledge | note set, course, learning collection |
| Project | Git repository, personal project, workspace |

The base Asset model should remain small. Domain-specific attributes belong to typed details or module-owned records.

## Core views

### 1. Overview

Purpose: answer “what changed and what deserves attention?”

Suggested sections:

- total assets by category;
- recently added;
- recently updated;
- currently active / in-progress assets;
- stale or needs-review assets;
- recent activity;
- pinned collections.

### 2. Library

Purpose: answer “what do I have?”

Requirements:

- global search;
- category filtering;
- tags;
- collections;
- saved filters later;
- grid/list/table views may be added progressively.

### 3. Asset detail

Each asset detail page should use a consistent shell:

```text
Header
  name / icon / category / status

Summary
  canonical fields

Domain details
  module-specific metadata

Relations
  uses / depends on / belongs to / hosted by / related to

Activity
  history of meaningful changes

Sources
  where metadata or discovery came from

Actions
  domain-specific deterministic actions
```

### 4. Relations

Purpose: answer “how is my digital world connected?”

Graph view is secondary to the library. It should visualize relationships already present in the data model rather than invent its own parallel data source.

### 5. Activity

Purpose: answer “what happened?”

Examples:

- media completed;
- software installed;
- subscription renewed;
- project archived;
- service endpoint changed;
- relationship created or removed.

## Design principles

### Durable before clever

A record imported today should still be understandable years later. Schema design and portable export matter more than live dashboards.

### Manual truth, automated suggestions

Automatic discovery can propose assets or relations, but the user remains the owner of canonical data.

### Progressive complexity

A media record should not require infrastructure concepts. A software asset can optionally have runtime state. Advanced capabilities appear only when relevant.

### One asset, many views

Do not duplicate the same thing across “apps”, “services”, and “graph” databases. Views should project from shared canonical assets and relations.
