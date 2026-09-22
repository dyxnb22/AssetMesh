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
    const limit = query?.limit ?? 50;
    const offset = query?.offset ?? 0;
    const items = this.assets.slice(offset, offset + limit);
    return {
      items,
      offset,
      limit,
      total: this.assets.length,
    };
  }
}
