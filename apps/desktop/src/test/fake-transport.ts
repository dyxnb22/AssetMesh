import { importPreviewFixture, mergePreviewFixture } from './query-fixtures';
import type { DesktopTransport } from '../features/library/transport';
import type {
  AppCapabilities,
  AppStatus,
  AssetDetailDto,
  AssetDetailsDto,
  AssetSummary,
  ClassifiedCandidateDto,
  LibraryQuery,
  LibrarySearchQuery,
  MediaCommand,
  MediaRecordDto,
  MediaStatusCountDto,
  MutationReceiptDto,
  NeighborViewDto,
  Page,
  RelationAttachPayload,
  RelationNeighborsQuery,
  RelationRemovePayload,
  RelationTraverseQuery,
  RelationViewDto,
  ServiceCommand,
  InfoCommand,
  ServiceRuntimeState,
  ServiceRuntimeLogLineDto,
  ServiceRuntimeLogsDto,
  ServiceRuntimeStatusDto,
  InfoRecordDto,
  ServiceRecordDto,
  SoftwareCommand,
  SoftwareRecordDto,
  TraversalViewDto,
  ActivityQuery,
  ActivityViewDto,
  DuplicateCandidateDto,
  DuplicateQuery,
  MergeApplyCommand,
  MergePreviewDto,
  DiscoveryReport,
  BackupEntry,
  BackupStatus,
  RestoreReceipt,
  ExportReceipt,
  ImportPreview,
  ImportReceipt,
} from '../features/library/types';

const INVERSE_MAP: Record<string, string> = {
  depends_on: 'dependency_of',
  dependency_of: 'depends_on',
  uses: 'used_by',
  used_by: 'uses',
  installed_via: 'installs',
  installs: 'installed_via',
  hosted_on: 'hosts',
  hosts: 'hosted_on',
  points_to: 'pointed_to_by',
  pointed_to_by: 'points_to',
  related_to: 'related_to',
};

const PRIMARY_MAP: Record<string, string> = {
  dependency_of: 'depends_on',
  used_by: 'uses',
  installs: 'installed_via',
  hosts: 'hosted_on',
  pointed_to_by: 'points_to',
};

function normalizeRelationFact(
  source: string,
  type: string,
  target: string
): { source: string; type: string; target: string } {
  if (PRIMARY_MAP[type]) {
    return {
      source: target,
      type: PRIMARY_MAP[type],
      target: source,
    };
  }
  if (type === 'related_to' && source > target) {
    return { source: target, type, target: source };
  }
  return { source, type, target };
}

/** Stateful UI double for common CRUD; complex query decisions use canned DTOs.
 * It does not validate domain correctness. Use the real Rust contracts for that.
 */
export class FakeDesktopTransport implements DesktopTransport {
  status: AppStatus = { status: 'ready', db_path: ':memory:' };
  capabilities: AppCapabilities = {
    version: '0.3.0',
    modules: ['media', 'software', 'services', 'info'],
    asset_kinds: ['media.anime', 'software.app', 'service.saas', 'service.local', 'info.item'],
    relation_types: [
      'depends_on',
      'dependency_of',
      'uses',
      'used_by',
      'installed_via',
      'installs',
      'hosted_on',
      'hosts',
      'points_to',
      'pointed_to_by',
      'related_to',
    ],
    storable_relation_types: [
      'depends_on',
      'uses',
      'installed_via',
      'hosted_on',
      'points_to',
      'related_to',
    ],
    features: {
      runtime_enrichment: false,
      projects: false,
      agent_capabilities: false,
      knowledge_collections: false,
      local_service_runtime: true,
    },
  };
  assets: AssetSummary[] = [];
  activityEvents: ActivityViewDto[] = [];

  // Runtime double for the local-service page: a plain state machine the UI
  // reacts to. The real spawn/stop/cleanup behavior is proven by Rust
  // contracts against actual processes, not reproduced here.
  runtimeStates = new Map<string, ServiceRuntimeState>();
  runtimeLogsByAsset = new Map<string, ServiceRuntimeLogLineDto[]>();
  runtimeStartFailures = new Map<string, { category?: string; message: string }>();
  private runtimeLogSeq = 0;

  constructor(assets: AssetSummary[] = []) {
    this.assets = assets.map((asset) => ({ ...asset, revision: asset.revision ?? 1 }));
    for (const a of this.assets) {
      const mod = a.kind.split('.')[0] || 'asset';
      this.activityEvents.push({
        id: 'act-' + (this.activityEvents.length + 1),
        event_type: `${mod}.created`,
        module: mod,
        occurred_at: a.updated_at || new Date().toISOString(),
        actor: 'user',
        asset_id: a.id,
        asset_name: a.name,
        payload: { summary: a.subtitle },
      });
    }
  }

  recordActivity(
    event_type: string,
    asset_id?: string,
    asset_name?: string,
    payload: Record<string, unknown> = {},
    module?: string
  ): void {
    const event: ActivityViewDto = {
      id: 'act-' + (this.activityEvents.length + 1),
      event_type,
      module: module ?? (event_type.split('.')[0] || null),
      occurred_at: new Date().toISOString(),
      actor: 'user',
      asset_id: asset_id ?? null,
      asset_name: asset_name ?? null,
      payload,
    };
    this.activityEvents.unshift(event);
  }

  async getCapabilities(): Promise<AppCapabilities> {
    return this.capabilities;
  }

  async getStatus(): Promise<AppStatus> {
    return this.status;
  }

  async init(dbPath?: string): Promise<AppStatus> {
    this.status = { status: 'ready', db_path: dbPath ?? 'app-data://assetmesh.db' };
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
      filtered = filtered.filter((a) =>
        query.modules!.some((m) =>
          m === 'services' ? a.kind.startsWith('service.') : a.kind.startsWith(m)
        )
      );
    }

    // Kinds
    if (query?.kinds && query.kinds.length > 0) {
      filtered = filtered.filter((a) => query.kinds!.includes(a.kind));
    }

    // Media watch-status filter: only rows with matching typed media details
    if (query?.media_status) {
      filtered = filtered.filter(
        (a) => a.details?.module === 'media' && a.details.status === query.media_status
      );
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
      items: structuredClone(items),
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
      filtered = filtered.filter((a) =>
        query.modules!.some((m) =>
          m === 'services' ? a.kind.startsWith('service.') : a.kind.startsWith(m)
        )
      );
    }

    // Kinds
    if (query.kinds && query.kinds.length > 0) {
      filtered = filtered.filter((a) => query.kinds!.includes(a.kind));
    }

    // Tags
    if (query.tags && query.tags.length > 0) {
      filtered = filtered.filter((a) => query.tags!.every((t) => a.tags.includes(t)));
    }

    if (query.media_status) {
      filtered = filtered.filter((asset) => asset.details?.module === 'media' && asset.details.status === query.media_status);
    }
    // Sort
    const sort = query.sort ?? 'updated_desc';
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
    const items = filtered.slice(offset, offset + limit);

    return {
      items: structuredClone(items),
      offset,
      limit,
      total,
    };
  }

  async mediaStatusCounts(query?: LibraryQuery): Promise<MediaStatusCountDto[]> {
    // Same filters as listAssets but without the status filter itself, so the
    // segmented control shows what each choice would return.
    const page = await this.listAssets({
      ...query,
      media_status: undefined,
      limit: 10_000,
      offset: 0,
    });
    const counts = new Map<string, number>();
    for (const asset of page.items) {
      if (asset.details?.module === 'media') {
        counts.set(asset.details.status, (counts.get(asset.details.status) ?? 0) + 1);
      }
    }
    return [...counts.entries()]
      .map(([status, count]) => ({ status, count }))
      .sort((a, b) => a.status.localeCompare(b.status));
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
      // Honor seeded typed details (e.g. a local service) before falling back
      // to canned subscription data; the kind suffix is the service type.
      details = summary.details && summary.details.module === 'services'
        ? summary.details
        : {
            module: 'services',
            asset_id: summary.id,
            service_type: summary.kind.split('.')[1] || 'saas',
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
      revision: summary.revision ?? 1,
      created_at: summary.updated_at,
      updated_at: summary.updated_at,
      archived_at: summary.lifecycle === 'archived' ? summary.updated_at : null,
      merged_into: null,
      details: summary.details ?? details,
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

  candidates: ClassifiedCandidateDto[] = [
    {
      candidate: {
        provider: 'brew',
        display_name: 'ripgrep',
        category: 'cli',
        install_source: 'homebrew_formula',
        version: '14.1.0',
        install_location: '/opt/homebrew/bin/rg',
        executable_path: '/opt/homebrew/bin/rg',
        external_refs: [{ namespace: 'homebrew_formula', external_id: 'ripgrep' }],
      },
      disposition: 'new',
      matched_asset_ids: [],
      message: null,
    },
    {
      candidate: {
        provider: 'macos_apps',
        display_name: 'Visual Studio Code',
        category: 'application',
        install_source: 'macos_app',
        version: '1.85.0',
        install_location: '/Applications/Visual Studio Code.app',
        executable_path: '/Applications/Visual Studio Code.app/Contents/MacOS/Electron',
        external_refs: [{ namespace: 'macos_bundle', external_id: 'com.microsoft.VSCode' }],
      },
      disposition: 'new',
      matched_asset_ids: [],
      message: null,
    },
  ];

  async softwareDiscover(): Promise<DiscoveryReport> {
    return { candidates: [...this.candidates], completed_sources: ['macos_applications', 'homebrew', 'npm_global', 'pipx'], failed_sources: [] };
  }

  async softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto> {
    if (command.action === 'create') {
      if (!command.name || !command.name.trim()) {
        const error = new Error('software name must not be empty') as Error & { category?: string };
        error.category = 'invalid_input';
        throw error;
      }
      const newId = 'asset-software-' + (this.assets.length + 1);
      const now = new Date().toISOString();
      const kind = `software.${command.category === 'application' ? 'app' : command.category}`;

      const softwareRecord: SoftwareRecordDto = {
        module: 'software',
        asset_id: newId,
        category: command.category,
        install_source: command.install_source ?? 'unknown',
        version: command.version ?? null,
        install_location: command.install_location ?? null,
        executable_path: command.executable_path ?? null,
        purpose: command.purpose ?? null,
        notes: command.notes ?? null,
        architecture: command.architecture ?? null,
        discovered_at: null,
        installed_at: null,
      };

      const newDetail: AssetDetailDto = {
        id: newId,
        kind,
        name: command.name.trim(),
        summary: command.summary ?? null,
        lifecycle: 'active',
        revision: 1,
        created_at: now,
        updated_at: now,
        archived_at: null,
        merged_into: null,
        details: softwareRecord,
        tags: command.tags || [],
        external_refs: [],
      };

      this.details.set(newId, newDetail);
      this.assets.unshift({
        id: newId,
        kind,
        name: newDetail.name,
        lifecycle: 'active',
        subtitle: newDetail.summary,
        tags: newDetail.tags,
        revision: 1,
        updated_at: now,
      });

      return {
        operation: 'software.create',
        asset_ids: [newId],
        revision: 1,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'adopt_candidate') {
      const c = command.candidate;
      const targetId =
        command.target && command.target !== 'auto' && command.target !== 'create_new'
          ? command.target
          : null;

      let assetId = targetId;
      let isNew = false;
      const now = new Date().toISOString();

      if (!assetId) {
        assetId = 'asset-software-' + (this.assets.length + 1);
        isNew = true;
      }

      const existingDetail = !isNew ? await this.getAsset(assetId) : null;
      if (existingDetail && command.expected_revision === undefined) {
        throw { category: 'invalid_input', message: 'expected_revision is required when adopting into an existing asset' };
      }
      if (existingDetail && existingDetail.revision !== command.expected_revision) {
        throw { category: 'stale_revision', message: `expected revision ${command.expected_revision}, found ${existingDetail.revision}` };
      }
      const rev = existingDetail ? existingDetail.revision + 1 : 1;
      const kind = `software.${c.category === 'application' ? 'app' : c.category}`;

      // User overrides: keep existing or use provided, never overwrite user purpose/notes with empty
      const purpose =
        command.purpose !== undefined && command.purpose !== null
          ? command.purpose
          : existingDetail
            ? (existingDetail.details as SoftwareRecordDto).purpose
            : null;

      const notes =
        command.notes !== undefined && command.notes !== null
          ? command.notes
          : existingDetail
            ? (existingDetail.details as SoftwareRecordDto).notes
            : null;

      const softwareRecord: SoftwareRecordDto = {
        module: 'software',
        asset_id: assetId,
        category: c.category,
        install_source: c.install_source,
        version: c.version ?? null,
        install_location: c.install_location ?? null,
        executable_path: c.executable_path ?? null,
        purpose,
        notes,
        architecture: null,
        discovered_at: now,
        installed_at: null,
      };

      const newDetail: AssetDetailDto = {
        id: assetId,
        kind,
        name: c.display_name,
        summary: existingDetail ? existingDetail.summary : null,
        lifecycle: 'active',
        revision: rev,
        created_at: existingDetail ? existingDetail.created_at : now,
        updated_at: now,
        archived_at: null,
        merged_into: null,
        details: softwareRecord,
        tags: command.tags || (existingDetail ? existingDetail.tags : []),
        external_refs: c.external_refs.map((r) => ({
          namespace: r.namespace,
          external_id: r.external_id,
          source_url: null,
        })),
      };

      this.details.set(assetId, newDetail);

      if (isNew) {
        this.assets.unshift({
          id: assetId,
          kind,
          name: newDetail.name,
          lifecycle: 'active',
          subtitle: newDetail.summary,
          tags: newDetail.tags,
          revision: rev,
          updated_at: now,
        });
      } else {
        const summary = this.assets.find((a) => a.id === assetId);
        if (summary) {
          summary.name = newDetail.name;
          summary.revision = rev;
          summary.updated_at = now;
        }
      }

      return {
        operation: 'software.adopt',
        asset_ids: [assetId],
        revision: rev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'archive') {
      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        lifecycle: 'archived',
        revision: updatedRev,
        archived_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };

      this.details.set(command.asset_id, updatedDetail);
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (summary) {
        summary.lifecycle = 'archived';
        summary.revision = updatedRev;
        summary.updated_at = updatedDetail.updated_at;
      }

      return {
        operation: 'asset.archive',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

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
      summary.revision = updatedRev;
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

  async mediaCommand(command: MediaCommand): Promise<MutationReceiptDto> {
    if (command.action === 'create') {
      if (!command.title || !command.title.trim()) {
        const error = new Error('media title must not be empty') as Error & { category?: string };
        error.category = 'invalid_input';
        throw error;
      }
      const newId = 'asset-media-' + (this.assets.length + 1);
      const now = new Date().toISOString();
      const kind = `media.${command.media_type}`;
      const status = command.status || 'planned';

      const progress = command.progress_unit
        ? {
            unit: command.progress_unit,
            current: command.progress_current ?? 0,
            total: command.progress_total ?? null,
          }
        : null;

      const mediaRecord: MediaRecordDto = {
        module: 'media',
        asset_id: newId,
        media_type: command.media_type,
        status,
        rating: command.rating ?? null,
        year: command.year ?? null,
        platform: command.platform ?? null,
        progress,
        notes: command.notes ?? null,
        started_at: status === 'in_progress' ? now : null,
        completed_at: status === 'completed' ? now : null,
      };

      const newDetail: AssetDetailDto = {
        id: newId,
        kind,
        name: command.title.trim(),
        summary: command.summary ?? null,
        lifecycle: 'active',
        revision: 1,
        created_at: now,
        updated_at: now,
        archived_at: null,
        merged_into: null,
        details: mediaRecord,
        tags: command.tags || [],
        external_refs: [],
      };

      this.details.set(newId, newDetail);
      this.assets.unshift({
        id: newId,
        kind,
        name: newDetail.name,
        lifecycle: 'active',
        subtitle: newDetail.summary,
        tags: newDetail.tags,
        revision: 1,
        updated_at: now,
        details: mediaRecord,
      });

      return {
        operation: 'media.create',
        asset_ids: [newId],
        revision: 1,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'update_metadata') {
      const currentDetail = await this.getAsset(command.asset_id);
      if (
        command.title === undefined &&
        command.summary === undefined &&
        command.year === undefined &&
        command.platform === undefined &&
        command.notes === undefined
      ) {
        return {
          operation: 'media.update_metadata',
          asset_ids: [command.asset_id],
          revision: command.expected_revision ?? currentDetail.revision,
          changed: false,
          warnings: ['No-op: no fields were updated'],
        };
      }

      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }

      const updatedRev = currentDetail.revision + 1;
      const mediaRecord = { ...(currentDetail.details as MediaRecordDto) };
      if (command.year !== undefined) mediaRecord.year = command.year;
      if (command.platform !== undefined) mediaRecord.platform = command.platform;
      if (command.notes !== undefined) mediaRecord.notes = command.notes;

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        name: command.title !== undefined && command.title !== null ? command.title : currentDetail.name,
        summary: command.summary !== undefined ? command.summary : currentDetail.summary,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: mediaRecord,
      };

      this.details.set(command.asset_id, updatedDetail);
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (summary) {
        summary.name = updatedDetail.name;
        summary.subtitle = updatedDetail.summary;
        summary.revision = updatedRev;
        summary.updated_at = updatedDetail.updated_at;
      }

      return {
        operation: 'media.update_metadata',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'transition_status') {
      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const mediaRecord = {
        ...(currentDetail.details as MediaRecordDto),
        status: command.status,
      };

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: mediaRecord,
      };

      this.details.set(command.asset_id, updatedDetail);
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (summary) {
        summary.revision = updatedRev;
        summary.updated_at = updatedDetail.updated_at;
        summary.details = mediaRecord;
      }

      return {
        operation: `media.transition.${command.status}`,
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'update_progress') {
      if (command.current !== undefined && command.current !== null && command.current < 0) {
        const error = new Error('progress_current must not be negative') as Error & {
          category?: string;
        };
        error.category = 'invalid_input';
        throw error;
      }

      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const mediaRecord = {
        ...(currentDetail.details as MediaRecordDto),
        progress: {
          unit: command.unit || (currentDetail.details as MediaRecordDto).progress?.unit || 'episodes',
          current: command.current ?? (currentDetail.details as MediaRecordDto).progress?.current ?? 0,
          total: command.total !== undefined ? command.total : (currentDetail.details as MediaRecordDto).progress?.total ?? null,
        },
      };

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: mediaRecord,
      };

      this.details.set(command.asset_id, updatedDetail);

      const progressSummary = this.assets.find((asset) => asset.id === command.asset_id);
      if (progressSummary) {
        progressSummary.revision = updatedRev;
        progressSummary.details = mediaRecord;
        progressSummary.updated_at = updatedDetail.updated_at;
      }

      return {
        operation: 'media.update_progress',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'rate') {
      if (command.rating < 0 || command.rating > 10) {
        const error = new Error('rating must be between 0 and 10') as Error & { category?: string };
        error.category = 'invalid_input';
        throw error;
      }

      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const mediaRecord = {
        ...(currentDetail.details as MediaRecordDto),
        rating: command.rating,
      };

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: mediaRecord,
      };

      this.details.set(command.asset_id, updatedDetail);

      const ratingSummary = this.assets.find((asset) => asset.id === command.asset_id);
      if (ratingSummary) {
        ratingSummary.revision = updatedRev;
        ratingSummary.details = mediaRecord;
        ratingSummary.updated_at = updatedDetail.updated_at;
      }

      return {
        operation: 'media.rate',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'archive') {
      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        lifecycle: 'archived',
        revision: updatedRev,
        archived_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };

      this.details.set(command.asset_id, updatedDetail);
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (summary) {
        summary.lifecycle = 'archived';
        summary.revision = updatedRev;
        summary.updated_at = updatedDetail.updated_at;
      }

      return {
        operation: 'asset.archive',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    throw new Error('Unsupported media command action');
  }

  async infoCommand(command: InfoCommand): Promise<MutationReceiptDto> {
    const now = new Date().toISOString();
    if (command.action === 'batch_create') {
      if (command.items.length < 1 || command.items.length > 500 ||
          command.items.some((item) => !item.name.trim() || !item.value.trim())) {
        throw new Error('Information import must contain 1 to 500 valid items.');
      }
      const ids: string[] = [];
      for (const item of command.items) {
        const receipt = await this.infoCommand({ action: 'create', ...item });
        ids.push(...receipt.asset_ids);
      }
      return { operation: 'info.batch_create', asset_ids: ids, revision: null, changed: true, warnings: [] };
    }
    if (command.action === 'create') {
      if (!command.name.trim() || !command.value.trim()) throw new Error('Name and value are required.');
      const id = 'asset-info-' + (this.assets.length + 1);
      const record: InfoRecordDto = {
        module: 'info', asset_id: id, info_type: command.info_type,
        value: command.value.trim(), notes: command.notes?.trim() || null,
      };
      this.assets.unshift({ id, kind: 'info.item', name: command.name.trim(), lifecycle: 'active',
        revision: 1, subtitle: command.info_type, tags: command.tags ?? [], updated_at: now });
      this.details.set(id, { id, kind: 'info.item', name: command.name.trim(), summary: null,
        lifecycle: 'active', revision: 1, created_at: now, updated_at: now,
        archived_at: null, merged_into: null, details: record, tags: command.tags ?? [], external_refs: [] });
      return { operation: 'info.create', asset_ids: [id], revision: 1, changed: true, warnings: [] };
    }
    const detail = await this.getAsset(command.asset_id);
    if (detail.revision !== command.expected_revision) throw new Error('Stale revision');
    const revision = detail.revision + 1;
    if (command.action === 'archive') {
      detail.lifecycle = 'archived';
      detail.archived_at = now;
    } else {
      detail.name = command.name.trim();
      detail.details = { module: 'info', asset_id: detail.id,
        info_type: command.info_type, value: command.value.trim(), notes: command.notes?.trim() || null };
      if (command.tags) detail.tags = command.tags;
    }
    detail.revision = revision;
    detail.updated_at = now;
    this.details.set(detail.id, detail);
    const summary = this.assets.find((item) => item.id === detail.id);
    if (summary) {
      summary.name = detail.name;
      summary.lifecycle = detail.lifecycle;
      summary.subtitle = detail.details.module === 'info' ? detail.details.info_type : null;
      summary.tags = detail.tags;
      summary.revision = revision;
      summary.updated_at = now;
    }
    return { operation: command.action === 'archive' ? 'info.archive' : 'info.update',
      asset_ids: [detail.id], revision, changed: true, warnings: [] };
  }

  async serviceCommand(command: ServiceCommand): Promise<MutationReceiptDto> {
    if (command.action === 'create') {
      if (!command.name || !command.name.trim()) {
        const error = new Error('service name must not be empty') as Error & { category?: string };
        error.category = 'invalid_input';
        throw error;
      }

      // Money pairing
      if ((command.cost && !command.currency) || (!command.cost && command.currency)) {
        const error = new Error('cost and currency must be both present or both absent') as Error & {
          category?: string;
        };
        error.category = 'invalid_input';
        throw error;
      }

      let costMinor: number | null = null;
      if (command.cost) {
        const num = parseFloat(command.cost);
        if (isNaN(num) || num < 0) {
          const error = new Error('invalid cost') as Error & { category?: string };
          error.category = 'invalid_input';
          throw error;
        }
        costMinor = Math.round(num * 100);
      }

      const newId = 'asset-service-' + (this.assets.length + 1);
      const now = new Date().toISOString();
      const kind = `service.${command.service_type}`;

      const serviceRecord: ServiceRecordDto = {
        module: 'services',
        asset_id: newId,
        service_type: command.service_type,
        provider: command.provider ?? null,
        account_label: command.account_label ?? null,
        endpoint_url: command.endpoint_url ?? null,
        dashboard_url: command.dashboard_url ?? null,
        domain_name: command.domain_name ?? null,
        plan: command.plan ?? null,
        cost_minor: costMinor,
        currency: command.currency ?? null,
        billing_cadence: command.billing_cadence ?? null,
        renews_at: command.renews_at ?? null,
        expires_at: command.expires_at ?? null,
        auto_renew: command.auto_renew ?? null,
        notes: command.notes ?? null,
        project_dir: command.project_dir ?? null,
        start_command: command.start_command ?? null,
        stop_command: command.stop_command ?? null,
      };

      const newDetail: AssetDetailDto = {
        id: newId,
        kind,
        name: command.name.trim(),
        summary: command.summary ?? null,
        lifecycle: 'active',
        revision: 1,
        created_at: now,
        updated_at: now,
        archived_at: null,
        merged_into: null,
        details: serviceRecord,
        tags: command.tags || [],
        external_refs: [],
      };

      this.details.set(newId, newDetail);
      this.assets.unshift({
        id: newId,
        kind,
        name: newDetail.name,
        lifecycle: 'active',
        subtitle: newDetail.summary,
        tags: newDetail.tags,
        revision: 1,
        updated_at: now,
      });

      return {
        operation: 'service.create',
        asset_ids: [newId],
        revision: 1,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'update') {
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (!summary) {
        const error = new Error(`Asset not found: ${command.asset_id}`) as Error & { category?: string };
        error.category = 'not_found';
        throw error;
      }

      if (
        command.name === undefined &&
        command.summary === undefined &&
        command.provider === undefined &&
        command.account_label === undefined &&
        command.endpoint_url === undefined &&
        command.dashboard_url === undefined &&
        command.domain_name === undefined &&
        command.plan === undefined &&
        command.cost === undefined &&
        command.currency === undefined &&
        command.billing_cadence === undefined &&
        command.renews_at === undefined &&
        command.expires_at === undefined &&
        command.auto_renew === undefined &&
        command.notes === undefined &&
        command.project_dir === undefined &&
        command.start_command === undefined && command.stop_command === undefined
      ) {
        return {
          operation: 'service.update',
          asset_ids: [command.asset_id],
          revision: command.expected_revision ?? 1,
          changed: false,
          warnings: ['No-op: no fields were updated'],
        };
      }

      const currentDetail = await this.getAsset(command.asset_id);

      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }

      const updatedRev = currentDetail.revision + 1;
      const rec = { ...(currentDetail.details as ServiceRecordDto) };

      if (command.provider !== undefined) rec.provider = command.provider;
      if (command.account_label !== undefined) rec.account_label = command.account_label;
      if (command.endpoint_url !== undefined) rec.endpoint_url = command.endpoint_url || null;
      if (command.dashboard_url !== undefined) rec.dashboard_url = command.dashboard_url;
      if (command.domain_name !== undefined) rec.domain_name = command.domain_name;
      if (command.plan !== undefined) rec.plan = command.plan;
      if (command.billing_cadence !== undefined) rec.billing_cadence = command.billing_cadence;
      if (command.renews_at !== undefined) rec.renews_at = command.renews_at;
      if (command.expires_at !== undefined) rec.expires_at = command.expires_at;
      if (command.auto_renew !== undefined) rec.auto_renew = command.auto_renew;
      if (command.notes !== undefined) rec.notes = command.notes;
      if (command.project_dir !== undefined) rec.project_dir = command.project_dir;
      if (command.start_command !== undefined) rec.start_command = command.start_command;
      if (command.stop_command !== undefined) rec.stop_command = command.stop_command;

      if (command.cost !== undefined || command.currency !== undefined) {
        if (command.cost && command.currency) {
          rec.cost_minor = Math.round(parseFloat(command.cost) * 100);
          rec.currency = command.currency.toUpperCase();
        } else if (!command.cost && !command.currency) {
          rec.cost_minor = null;
          rec.currency = null;
        } else {
          const error = new Error('cost and currency must be set together or cleared together') as Error & {
            category?: string;
          };
          error.category = 'invalid_input';
          throw error;
        }
      }

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        name: command.name !== undefined && command.name !== null ? command.name : currentDetail.name,
        summary: command.summary !== undefined ? command.summary : currentDetail.summary,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: rec,
      };

      this.details.set(command.asset_id, updatedDetail);
      summary.details = updatedDetail.details;
      summary.name = updatedDetail.name;
      summary.subtitle = updatedDetail.summary;
      summary.revision = updatedRev;
      summary.updated_at = updatedDetail.updated_at;

      return {
        operation: 'service.update',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'record_renewal') {
      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const rec = { ...(currentDetail.details as ServiceRecordDto) };

      rec.renews_at = command.next_renews_at || command.renews_at;
      if (command.next_expires_at) {
        rec.expires_at = command.next_expires_at;
      }
      if (command.cost && command.currency) {
        rec.cost_minor = Math.round(parseFloat(command.cost) * 100);
        rec.currency = command.currency.toUpperCase();
      }

      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        revision: updatedRev,
        updated_at: new Date().toISOString(),
        details: rec,
      };

      this.details.set(command.asset_id, updatedDetail);

      const renewalSummary = this.assets.find((asset) => asset.id === command.asset_id);
      if (renewalSummary) renewalSummary.revision = updatedRev;

      return {
        operation: 'service.record_renewal',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    if (command.action === 'archive') {
      const currentDetail = await this.getAsset(command.asset_id);
      if (command.expected_revision !== undefined && command.expected_revision !== null) {
        if (command.expected_revision !== currentDetail.revision) {
          const error = new Error(
            `stale revision: expected ${command.expected_revision}, actual ${currentDetail.revision}`
          ) as Error & { category?: string };
          error.category = 'stale_revision';
          throw error;
        }
      }
      const updatedRev = currentDetail.revision + 1;
      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        lifecycle: 'archived',
        revision: updatedRev,
        archived_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };

      this.details.set(command.asset_id, updatedDetail);
      const summary = this.assets.find((a) => a.id === command.asset_id);
      if (summary) {
        summary.lifecycle = 'archived';
        summary.revision = updatedRev;
        summary.updated_at = updatedDetail.updated_at;
      }

      return {
        operation: 'asset.archive',
        asset_ids: [command.asset_id],
        revision: updatedRev,
        changed: true,
        warnings: [],
      };
    }

    throw new Error('Unsupported service command action');
  }

  private runtimeStatusDto(assetId: string): ServiceRuntimeStatusDto {
    return {
      asset_id: assetId,
      state: this.runtimeStates.get(assetId) ?? 'stopped',
      pid: this.runtimeStates.get(assetId) === 'running' ? 4242 : null,
      started_at: this.runtimeStates.has(assetId) ? new Date().toISOString() : null,
      exit_code: null,
      exit_signal: null,
      error: null,
    };
  }

  /** Seeds the output a service shows after Start (or before it). */
  pushRuntimeLog(assetId: string, text: string, stream: 'stdout' | 'stderr' = 'stdout'): void {
    const lines = this.runtimeLogsByAsset.get(assetId) ?? [];
    lines.push({ seq: ++this.runtimeLogSeq, timestamp: new Date().toISOString(), stream, text });
    this.runtimeLogsByAsset.set(assetId, lines);
  }

  async serviceRuntimeStart(assetId: string): Promise<ServiceRuntimeStatusDto> {
    const failure = this.runtimeStartFailures.get(assetId);
    if (failure) {
      this.runtimeStates.set(assetId, 'failed');
      const error = new Error(failure.message) as Error & { category?: string };
      error.category = failure.category ?? 'invalid_input';
      throw error;
    }
    this.runtimeStates.set(assetId, 'running');
    return this.runtimeStatusDto(assetId);
  }

  async serviceRuntimeStop(assetId: string): Promise<ServiceRuntimeStatusDto | null> {
    if (!this.runtimeStates.has(assetId)) return null;
    this.runtimeStates.set(assetId, 'stopped');
    return this.runtimeStatusDto(assetId);
  }

  async serviceRuntimeRestart(assetId: string): Promise<ServiceRuntimeStatusDto> {
    await this.serviceRuntimeStop(assetId);
    return this.serviceRuntimeStart(assetId);
  }

  async serviceRuntimeStatus(assetId: string): Promise<ServiceRuntimeStatusDto> {
    return this.runtimeStatusDto(assetId);
  }

  async serviceRuntimeStatuses(): Promise<ServiceRuntimeStatusDto[]> {
    const ids = new Set([...this.runtimeStates.keys(), ...this.assets.filter((asset) => asset.kind === 'service.local' && asset.lifecycle === 'active').map((asset) => asset.id)]);
    return [...ids].map((id) => this.runtimeStatusDto(id));
  }

  async serviceRuntimeLogs(assetId: string, since = 0, runId?: string): Promise<ServiceRuntimeLogsDto> {
    return {
      run_id: `fixture-${assetId}`,
      lines: (this.runtimeLogsByAsset.get(assetId) ?? []).filter(line => runId !== `fixture-${assetId}` || line.seq > since),
      dropped: false,
    };
  }

  async serviceOpenPage(_assetId: string): Promise<void> {}

  storedRelations: Array<{
    id: string;
    source: string;
    target: string;
    type: string;
    note?: string | null;
    provenance: string;
    created_at: string;
  }> = [];

  async relationList(assetId: string): Promise<RelationViewDto[]> {
    const asset = this.assets.find((a) => a.id === assetId);
    if (!asset) {
      const err = new Error(`Asset not found: ${assetId}`) as Error & { category?: string };
      err.category = 'not_found';
      throw err;
    }

    const results: RelationViewDto[] = [];
    for (const r of this.storedRelations) {
      if (r.source === assetId) {
        const other = this.assets.find((a) => a.id === r.target);
        results.push({
          relation_id: r.id,
          other_asset_id: r.target,
          other_asset_name: other ? other.name : r.target,
          relation_type: r.type,
          outgoing: true,
          note: r.note,
          provenance: r.provenance,
          created_at: r.created_at,
        });
      } else if (r.target === assetId) {
        const other = this.assets.find((a) => a.id === r.source);
        const effectiveType = INVERSE_MAP[r.type] || r.type;
        results.push({
          relation_id: r.id,
          other_asset_id: r.source,
          other_asset_name: other ? other.name : r.source,
          relation_type: effectiveType,
          outgoing: false,
          note: r.note,
          provenance: r.provenance,
          created_at: r.created_at,
        });
      }
    }
    return results;
  }

  async relationNeighbors(query: RelationNeighborsQuery): Promise<NeighborViewDto[]> {
    const asset = this.assets.find((a) => a.id === query.asset_id);
    if (!asset) {
      const err = new Error(`Asset not found: ${query.asset_id}`) as Error & { category?: string };
      err.category = 'not_found';
      throw err;
    }

    if (asset.lifecycle === 'merged') {
      const err = new Error(
        `asset ${asset.id} was merged into another; request the surviving asset instead`
      ) as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const allViews = await this.relationList(query.asset_id);
    const direction = query.direction || 'both';
    let filtered = allViews;

    if (direction === 'outgoing') {
      filtered = filtered.filter((v) => v.outgoing);
    } else if (direction === 'incoming') {
      filtered = filtered.filter((v) => !v.outgoing);
    }

    if (query.relation_types && query.relation_types.length > 0) {
      const allowed = new Set(query.relation_types);
      filtered = filtered.filter(
        (v) => allowed.has(v.relation_type) || allowed.has(INVERSE_MAP[v.relation_type])
      );
    }

    const neighbors: NeighborViewDto[] = [];
    for (const edge of filtered) {
      const otherAsset = this.assets.find((a) => a.id === edge.other_asset_id);
      if (!otherAsset) continue;
      if (!query.include_archived && otherAsset.lifecycle === 'archived') continue;
      if (otherAsset.lifecycle === 'merged') continue;
      neighbors.push({
        asset: otherAsset,
        edge,
      });
    }

    return neighbors;
  }

  traversalResult: TraversalViewDto | null = null;

  async relationTraverse(query: RelationTraverseQuery): Promise<TraversalViewDto> {
    if (this.traversalResult) return this.traversalResult;
    const root = this.assets.find((asset) => asset.id === query.asset_id);
    if (!root) throw new Error(`Missing traversal fixture for ${query.asset_id}`);
    return { root, nodes: [], truncated: false };
  }

  async relationAttach(payload: RelationAttachPayload): Promise<MutationReceiptDto> {
    const source = this.assets.find((a) => a.id === payload.source_asset_id);
    const target = this.assets.find((a) => a.id === payload.target_asset_id);

    if (!source || !target) {
      const err = new Error('source or target asset not found') as Error & { category?: string };
      err.category = 'not_found';
      throw err;
    }

    if (payload.expected_source_revision !== undefined && payload.expected_source_revision !== null) {
      if (source.revision !== payload.expected_source_revision) {
        const err = new Error(
          `asset revision mismatch for source ${source.id}: expected ${payload.expected_source_revision}, found ${source.revision}`
        ) as Error & { category?: string };
        err.category = 'stale_revision';
        throw err;
      }
    }
    if (payload.expected_target_revision !== undefined && payload.expected_target_revision !== null) {
      if (target.revision !== payload.expected_target_revision) {
        const err = new Error(
          `asset revision mismatch for target ${target.id}: expected ${payload.expected_target_revision}, found ${target.revision}`
        ) as Error & { category?: string };
        err.category = 'stale_revision';
        throw err;
      }
    }

    if (source.lifecycle !== 'active' || target.lifecycle !== 'active') {
      const err = new Error('both assets must be active') as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const norm = normalizeRelationFact(
      payload.source_asset_id,
      payload.relation_type,
      payload.target_asset_id
    );

    const existing = this.storedRelations.find(
      (r) => r.source === norm.source && r.target === norm.target && r.type === norm.type
    );
    if (existing) {
      const err = new Error(
        `relation ${norm.type} between ${norm.source} and ${norm.target} already exists`
      ) as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const id = 'rel-' + (this.storedRelations.length + 1);
    this.storedRelations.push({
      id,
      source: norm.source,
      target: norm.target,
      type: norm.type,
      note: payload.note ?? null,
      provenance: 'manual',
      created_at: new Date().toISOString(),
    });

    const nextSourceRev = (source.revision ?? 1) + 1;
    const nextTargetRev = (target.revision ?? 1) + 1;
    source.revision = nextSourceRev;
    target.revision = nextTargetRev;
    const sDetail = this.details.get(source.id);
    if (sDetail) sDetail.revision = nextSourceRev;
    const tDetail = this.details.get(target.id);
    if (tDetail) tDetail.revision = nextTargetRev;

    return {
      operation: 'relation.attach',
      asset_ids: [payload.source_asset_id, payload.target_asset_id],
      revision: nextSourceRev,
      changed: true,
      warnings: [],
    };
  }

  async relationRemove(payload: RelationRemovePayload): Promise<MutationReceiptDto> {
    const idx = this.storedRelations.findIndex((r) => r.id === payload.relation_id);
    if (idx === -1) {
      const err = new Error(`Relation not found: ${payload.relation_id}`) as Error & {
        category?: string;
      };
      err.category = 'not_found';
      throw err;
    }

    const relation = this.storedRelations[idx];
    if (payload.context_asset_id !== relation.source && payload.context_asset_id !== relation.target) {
      throw { category: 'invalid_input', message: 'Context asset does not touch the relation.' };
    }
    const contextAsset = this.assets.find((a) => a.id === payload.context_asset_id);
    if (!contextAsset) {
      throw { category: 'not_found', message: 'Context asset was not found.' };
    }
    if (contextAsset.revision !== payload.expected_context_revision) {
      throw { category: 'stale_revision', message: `expected revision ${payload.expected_context_revision}, found ${contextAsset.revision}` };
    }
    for (const endpoint of [relation.source, relation.target]) {
      const asset = this.assets.find((item) => item.id === endpoint);
      if (asset) {
        asset.revision = (asset.revision ?? 1) + 1;
        const detail = this.details.get(endpoint);
        if (detail) detail.revision = asset.revision;
      }
    }

    this.storedRelations.splice(idx, 1);
    return {
      operation: 'relation.remove',
      asset_ids: [payload.context_asset_id],
      revision: contextAsset.revision,
      changed: true,
      warnings: [],
    };
  }

  async activityQuery(query?: ActivityQuery): Promise<Page<ActivityViewDto>> {
    let filtered = [...this.activityEvents];

    if (query?.asset_id) {
      filtered = filtered.filter((e) => e.asset_id === query.asset_id);
    }
    if (query?.modules && query.modules.length > 0) {
      filtered = filtered.filter((e) => e.module && query.modules!.includes(e.module));
    }
    // Resolved the way the backend resolves it: through the event's asset to
    // the asset's *current* kind. An event whose asset is gone has no kind and
    // therefore cannot satisfy a kind filter.
    if (query?.kinds && query.kinds.length > 0) {
      const kindOf = (id: string | null): string | null =>
        (id && this.assets.find((a) => a.id === id)?.kind) ?? null;
      filtered = filtered.filter((e) => {
        const kind = kindOf(e.asset_id);
        return kind !== null && query.kinds!.includes(kind);
      });
    }
    if (query?.event_types && query.event_types.length > 0) {
      filtered = filtered.filter((e) => query.event_types!.includes(e.event_type));
    }
    if (query?.actors && query.actors.length > 0) {
      filtered = filtered.filter((e) => query.actors!.includes(e.actor));
    }
    if (query?.since) {
      filtered = filtered.filter((e) => e.occurred_at >= query.since!);
    }
    if (query?.until) {
      filtered = filtered.filter((e) => e.occurred_at <= query.until!);
    }

    filtered.sort((a, b) => b.occurred_at.localeCompare(a.occurred_at) || b.id.localeCompare(a.id));

    const total = filtered.length;
    const offset = query?.offset ?? 0;
    const limit = query?.limit ?? 20;
    const items = filtered.slice(offset, offset + limit);

    return {
      items,
      offset,
      limit,
      total,
    };
  }

  duplicateMatches: DuplicateCandidateDto[] = [];
  mergePreviewOverrides: Partial<MergePreviewDto> = {};

  async duplicateCandidates(query?: DuplicateQuery): Promise<Page<DuplicateCandidateDto>> {
    const offset = query?.offset ?? 0;
    const limit = query?.limit ?? 20;
    return { items: this.duplicateMatches.slice(offset, offset + limit), total: this.duplicateMatches.length, offset, limit };
  }

  async mergePreview(winner_id: string, loser_id: string): Promise<MergePreviewDto> {
    const winner = this.assets.find((asset) => asset.id === winner_id);
    const loser = this.assets.find((asset) => asset.id === loser_id);
    if (!winner || !loser) throw new Error('Missing merge fixture assets');
    return mergePreviewFixture(winner, loser, this.mergePreviewOverrides);
  }

  async mergeApply(command: MergeApplyCommand): Promise<MutationReceiptDto> {
    const loser = this.assets.find((asset) => asset.id === command.loser_id)!;
    loser.lifecycle = 'merged';
    const loserDetail = this.details.get(loser.id);
    if (loserDetail) {
      loserDetail.lifecycle = 'merged';
      loserDetail.merged_into = command.winner_id;
      loserDetail.details = { module: 'merged_redirect', surviving_asset_id: command.winner_id };
    }
    const winner = this.assets.find((asset) => asset.id === command.winner_id)!;
    winner.revision = (winner.revision ?? 1) + 1;
    const winnerDetail = this.details.get(winner.id);
    if (winnerDetail) winnerDetail.revision = winner.revision;
    this.recordActivity('asset.merged', winner.id, winner.name, { loser_id: loser.id }, 'asset');
    return { operation: 'asset.merge', asset_ids: [winner.id, loser.id], revision: winner.revision, changed: true, warnings: [] };
  }

  // =========================================================================
  // Import / Export & Settings (P5-09)
  // =========================================================================

  public nextPickedDirectory: string | null | undefined = undefined;
  public nextImportPreview: ImportPreview | null = null;
  public nextImportReceipt: ImportReceipt | null = null;
  /// Every directory `portableImportApply` was actually called with, in order.
  /// Lets a test assert which bundle the UI committed to, not just that it did.
  public importApplyCalls: string[] = [];
  /// Every directory `portableImportPreview` was actually called with, in order.
  public importPreviewCalls: string[] = [];

  async pickDirectory(prompt?: string): Promise<string | null> {
    void prompt;
    if (this.nextPickedDirectory !== undefined) {
      return this.nextPickedDirectory;
    }
    return '/Users/diaoyuxuan/Downloads/assetmesh-export';
  }

  async portableExport(targetDir: string): Promise<ExportReceipt> {
    if (!targetDir || !targetDir.trim()) {
      const err = new Error('Target directory cannot be empty') as Error & { category?: string };
      err.category = 'invalid_input';
      throw err;
    }

    let mediaCount = 0;
    let softwareCount = 0;
    let serviceCount = 0;

    for (const d of this.details.values()) {
      if (d.details.module === 'media') mediaCount++;
      if (d.details.module === 'software') softwareCount++;
      if (d.details.module === 'services') serviceCount++;
    }

    const counts: Record<string, number> = {
      assets: this.assets.length,
      media: mediaCount,
      software: softwareCount,
      services: serviceCount,
      relations: this.storedRelations.length,
      external_refs: 0,
      tags: 0,
      activity: this.activityEvents.length,
    };

    const files = [
      'manifest.json',
      'assets.jsonl',
      'external_refs.jsonl',
      'activity.jsonl',
      'tags.json',
      'asset_tags.jsonl',
      'relations.jsonl',
      'modules/media.jsonl',
      'modules/software.jsonl',
      'modules/services.jsonl',
    ];

    return {
      target_dir: targetDir,
      format: 'assetmesh-portable-export',
      version: 1,
      app_version: '0.3.0',
      created_at: new Date().toISOString(),
      record_counts: counts,
      files,
    };
  }

  async portableImportPreview(sourceDir: string): Promise<ImportPreview> {
    this.importPreviewCalls.push(sourceDir);
    return this.nextImportPreview ?? importPreviewFixture(sourceDir);
  }

  async portableImportApply(sourceDir: string, _expectedFingerprint?: string): Promise<ImportReceipt> {
    this.importApplyCalls.push(sourceDir);
    return this.nextImportReceipt ?? {
      success: true,
      source_dir: sourceDir,
      applied_at: '2026-01-01T00:00:00Z',
      report: (this.nextImportPreview ?? importPreviewFixture(sourceDir)).dispositions,
    };
  }

  private backupEntries: BackupEntry[] = [];
  private preferences: Record<string, string> = {};
  private restorePending = false;

  async backupStatus(): Promise<BackupStatus> {
    return { directory: '/test/assetmesh.backups', entries: this.backupEntries, issues: [], storage_bytes: this.backupEntries.length * 1_048_576, budget_bytes: 268_435_456, last_error: null, restore_pending: this.restorePending };
  }
  async backupCreate(): Promise<BackupEntry> {
    const id = `backup-${this.backupEntries.length + 1}`;
    const entry: BackupEntry = { id, source_dir: `/test/assetmesh.backups/${id}`, created_at: new Date().toISOString(), kind: 'snapshot', reason: 'manual', asset_count: this.assets.length, contains_api_keys: false, fingerprint: id };
    this.backupEntries = [entry, ...this.backupEntries];
    return entry;
  }
  async backupPreview(sourceDir: string): Promise<BackupEntry> {
    const entry = this.backupEntries.find((item) => item.source_dir === sourceDir);
    if (!entry) throw { category: 'corrupt_data', message: 'Backup not found' };
    return entry;
  }
  async backupRestore(sourceDir: string, expectedFingerprint: string): Promise<RestoreReceipt> {
    const entry = await this.backupPreview(sourceDir);
    if (entry.fingerprint !== expectedFingerprint) throw { category: 'conflict', message: 'Backup changed since preview' };
    this.restorePending = true;
    return { db_path: '/test/restored.db', preferences: this.preferences, restart_required: true };
  }
  async backupPreferences(): Promise<Record<string, string>> { return this.preferences; }
  async startupTiming(): Promise<void> { /* Native startup observations are outside the UI fake. */ }
  async backupSavePreferences(preferences: Record<string, string>): Promise<void> { this.preferences = preferences; }
  async backupTick(): Promise<void> {}
  async backupExportCopy(targetDir: string): Promise<string> { return `${targetDir}/assetmesh-backup-copy`; }


}
