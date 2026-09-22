import type { DesktopTransport } from '../features/library/transport';
import type { AppCapabilities, AppStatus, AssetSummary, LibraryQuery, Page } from '../features/library/types';

export class FakeDesktopTransport implements DesktopTransport {
  status: AppStatus = { status: 'ready', db_path: ':memory:' };
  capabilities: AppCapabilities = {
    version: '0.3.0',
    modules: ['media', 'software', 'services'],
    asset_kinds: ['media.anime', 'software.app', 'service.saas'],
    relation_types: ['depends_on', 'uses'],
    storable_relation_types: ['depends_on', 'uses'],
    features: {
      runtime_enrichment: false,
      projects: false,
      agent_capabilities: false,
      knowledge_collections: false,
    },
  };
  assets: AssetSummary[] = [];

  constructor(assets: AssetSummary[] = []) {
    this.assets = assets;
  }

  async getCapabilities(): Promise<AppCapabilities> {
    return this.capabilities;
  }

  async getStatus(): Promise<AppStatus> {
    return this.status;
  }

  async init(dbPath: string): Promise<AppStatus> {
    this.status = { status: 'ready', db_path: dbPath };
    return this.status;
  }

  async listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>> {
    let filtered = [...this.assets];

    // Lifecycle
    const lifecycle = query?.lifecycle ?? 'active';
    if (lifecycle === 'active') {
      filtered = filtered.filter((a) => a.lifecycle === 'active');
    } else if (lifecycle === 'archived' || lifecycle === 'active_or_archived') {
      filtered = filtered.filter((a) => a.lifecycle === 'active' || a.lifecycle === 'archived');
    }

    // Module
    if (query?.modules && query.modules.length > 0) {
      filtered = filtered.filter((a) => query.modules!.some((m) => a.kind.startsWith(m)));
    }

    // Kinds
    if (query?.kinds && query.kinds.length > 0) {
      filtered = filtered.filter((a) => query.kinds!.includes(a.kind));
    }

    // Tags
    if (query?.tags && query.tags.length > 0) {
      filtered = filtered.filter((a) => query.tags!.every((t) => a.tags.includes(t)));
    }

    // Sort
    const sort = query?.sort ?? 'updated_desc';
    filtered.sort((a, b) => {
      switch (sort) {
        case 'name_asc':
          return a.name.localeCompare(b.name);
        case 'name_desc':
          return b.name.localeCompare(a.name);
        case 'kind_asc':
          return a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name);
        case 'updated_asc':
          return a.updated_at.localeCompare(b.updated_at);
        case 'updated_desc':
        default:
          return b.updated_at.localeCompare(a.updated_at);
      }
    });

    const total = filtered.length;
    const limit = query?.limit ?? 50;
    const offset = query?.offset ?? 0;
    const items = filtered.slice(offset, offset + limit);

    return {
      items,
      offset,
      limit,
      total,
    };
  }
}
