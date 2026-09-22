import type { DesktopTransport } from '../features/library/transport';
import type {
  AppCapabilities,
  AppStatus,
  AssetDetailDto,
  AssetDetailsDto,
  AssetSummary,
  LibraryQuery,
  LibrarySearchQuery,
  MutationReceiptDto,
  Page,
  SoftwareCommand,
  SoftwareRecordDto,
} from '../features/library/types';

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

  async searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>> {
    const text = query.text.trim().toLowerCase();
    const limit = query.limit ?? 50;
    const offset = query.offset ?? 0;

    if (!text) {
      return {
        items: [],
        offset,
        limit,
        total: 0,
      };
    }

    let filtered = [...this.assets];

    // Merged tombstones are never searchable
    filtered = filtered.filter((a) => a.lifecycle !== 'merged');

    // Lifecycle
    const lifecycle = query.lifecycle ?? 'active';
    if (lifecycle === 'active') {
      filtered = filtered.filter((a) => a.lifecycle === 'active');
    } else if (lifecycle === 'archived' || lifecycle === 'active_or_archived') {
      filtered = filtered.filter((a) => a.lifecycle === 'active' || a.lifecycle === 'archived');
    }

    // Text matching (name, subtitle, or tags)
    filtered = filtered.filter(
      (a) =>
        a.name.toLowerCase().includes(text) ||
        (a.subtitle && a.subtitle.toLowerCase().includes(text)) ||
        a.tags.some((t) => t.toLowerCase().includes(text))
    );

    // Module
    if (query.modules && query.modules.length > 0) {
      filtered = filtered.filter((a) => query.modules!.some((m) => a.kind.startsWith(m)));
    }

    // Kinds
    if (query.kinds && query.kinds.length > 0) {
      filtered = filtered.filter((a) => query.kinds!.includes(a.kind));
    }

    // Tags
    if (query.tags && query.tags.length > 0) {
      filtered = filtered.filter((a) => query.tags!.every((t) => a.tags.includes(t)));
    }

    const total = filtered.length;
    const items = filtered.slice(offset, offset + limit);

    return {
      items,
      offset,
      limit,
      total,
    };
  }

  details: Map<string, AssetDetailDto> = new Map();

  async getAsset(id: string): Promise<AssetDetailDto> {
    const existing = this.details.get(id);
    if (existing) return existing;

    const summary = this.assets.find((a) => a.id === id);
    if (!summary) {
      throw new Error(`Asset not found: ${id}`);
    }

    let details: AssetDetailsDto;
    if (summary.kind.startsWith('media')) {
      details = {
        module: 'media',
        asset_id: summary.id,
        media_type: summary.kind.split('.')[1] || 'anime',
        status: 'completed',
        rating: 9.5,
        year: 2024,
        platform: 'Streaming',
        progress: { unit: 'episodes', current: 28, total: 28 },
        notes: 'Sample media record notes',
        started_at: '2024-01-01T00:00:00Z',
        completed_at: '2024-03-01T00:00:00Z',
      };
    } else if (summary.kind.startsWith('software')) {
      details = {
        module: 'software',
        asset_id: summary.id,
        category: 'tool',
        version: '1.0.0',
        install_location: '/usr/local/bin',
        executable_path: '/usr/local/bin/' + summary.name.toLowerCase(),
        purpose: 'Productivity tool',
        notes: 'Installed via package manager',
        architecture: 'arm64',
      };
    } else if (summary.kind.startsWith('service')) {
      details = {
        module: 'services',
        asset_id: summary.id,
        service_type: 'saas',
        provider: 'Cloud Provider',
        plan: 'Standard',
        cost_minor: 1200,
        currency: 'USD',
        dashboard_url: 'https://example.com/dashboard',
        auto_renew: true,
      };
    } else {
      details = {
        module: 'unknown',
      };
    }

    return {
      id: summary.id,
      kind: summary.kind,
      name: summary.name,
      summary: summary.subtitle,
      lifecycle: summary.lifecycle,
      revision: 1,
      created_at: summary.updated_at,
      updated_at: summary.updated_at,
      archived_at: summary.lifecycle === 'archived' ? summary.updated_at : null,
      merged_into: null,
      details,
      tags: summary.tags,
      external_refs: [
        {
          namespace: 'system',
          external_id: summary.name.toLowerCase().replace(/\s+/g, '-'),
          source_url: 'https://example.com/ref',
        },
      ],
    };
  }

  async softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto> {
    if (command.action === 'update_metadata') {
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (!summary) {
        const error = new Error(`Asset not found: ${command.asset_id}`) as Error & { category?: string };
        error.category = 'not_found';
        throw error;
      }

      // Check no-op
      if (
        command.name === undefined &&
        command.summary === undefined &&
        command.version === undefined &&
        command.install_location === undefined &&
        command.executable_path === undefined &&
        command.purpose === undefined &&
        command.notes === undefined &&
        command.architecture === undefined
      ) {
        return {
          operation: 'software.update_metadata',
          asset_ids: [command.asset_id],
          revision: command.expected_revision ?? 1,
          changed: false,
          warnings: ['No-op: no fields were updated'],
        };
      }

      // Check validation
      if (command.name !== undefined && (!command.name || !command.name.trim())) {
        const error = new Error('software name must not be empty') as Error & { category?: string };
        error.category = 'invalid_input';
        throw error;
      }

      // Get current detail
      const currentDetail = await this.getAsset(command.asset_id);

      // Check stale revision
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }

      // Apply updates
      const updatedRev = currentDetail.revision + 1;
      const updatedSoftware = { ...(currentDetail.details as SoftwareRecordDto) };
      if (command.purpose !== undefined) updatedSoftware.purpose = command.purpose;
      if (command.notes !== undefined) updatedSoftware.notes = command.notes;
      if (command.version !== undefined) updatedSoftware.version = command.version;
      if (command.install_location !== undefined)
        updatedSoftware.install_location = command.install_location;
      if (command.executable_path !== undefined)
        updatedSoftware.executable_path = command.executable_path;
      if (command.architecture !== undefined)
        updatedSoftware.architecture = command.architecture;

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        name:
          command.name !== undefined && command.name !== null
            ? command.name
            : currentDetail.name,
        summary: command.summary !== undefined ? command.summary : currentDetail.summary,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: updatedSoftware,
      };

      this.details.set(command.asset_id, updatedDetail);

      // Also update summary
      summary.name = updatedDetail.name;
      summary.subtitle = updatedDetail.summary;
      summary.updated_at = updatedDetail.updated_at;

      return {
        operation: 'software.update_metadata',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    throw new Error('Unsupported software command action');
  }
}
