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
  MutationReceiptDto,
  NeighborViewDto,
  Page,
  RelationAttachPayload,
  RelationNeighborsQuery,
  RelationPathHopDto,
  RelationRemovePayload,
  RelationTraverseQuery,
  RelationViewDto,
  ServiceCommand,
  ServiceRecordDto,
  SoftwareCommand,
  SoftwareRecordDto,
  TraversalNodeDto,
  TraversalViewDto,
  ActivityQuery,
  ActivityViewDto,
  DuplicateCandidateDto,
  DuplicateQuery,
  MergeApplyCommand,
  MergePreviewDto,
  AppSettings,
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

export class FakeDesktopTransport implements DesktopTransport {
  status: AppStatus = { status: 'ready', db_path: ':memory:' };
  capabilities: AppCapabilities = {
    version: '0.3.0',
    modules: ['media', 'software', 'services'],
    asset_kinds: ['media.anime', 'software.app', 'service.saas'],
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
    },
  };
  assets: AssetSummary[] = [];
  activityEvents: ActivityViewDto[] = [];

  constructor(assets: AssetSummary[] = []) {
    this.assets = [...assets];
    for (const a of assets) {
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

  async softwareDiscover(): Promise<ClassifiedCandidateDto[]> {
    return [...this.candidates];
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
          updated_at: now,
        });
      } else {
        const summary = this.assets.find((a) => a.id === assetId);
        if (summary) {
          summary.name = newDetail.name;
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
        updated_at: now,
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
        summary.updated_at = updatedDetail.updated_at;
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
        command.notes === undefined
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
      if (command.endpoint_url !== undefined) rec.endpoint_url = command.endpoint_url;
      if (command.dashboard_url !== undefined) rec.dashboard_url = command.dashboard_url;
      if (command.domain_name !== undefined) rec.domain_name = command.domain_name;
      if (command.plan !== undefined) rec.plan = command.plan;
      if (command.billing_cadence !== undefined) rec.billing_cadence = command.billing_cadence;
      if (command.renews_at !== undefined) rec.renews_at = command.renews_at;
      if (command.expires_at !== undefined) rec.expires_at = command.expires_at;
      if (command.auto_renew !== undefined) rec.auto_renew = command.auto_renew;
      if (command.notes !== undefined) rec.notes = command.notes;

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
      summary.name = updatedDetail.name;
      summary.subtitle = updatedDetail.summary;
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

  async relationTraverse(query: RelationTraverseQuery): Promise<TraversalViewDto> {
    const root = this.assets.find((a) => a.id === query.asset_id);
    if (!root) {
      const err = new Error(`Asset not found: ${query.asset_id}`) as Error & { category?: string };
      err.category = 'not_found';
      throw err;
    }

    if (root.lifecycle === 'merged') {
      const err = new Error(
        `asset ${root.id} was merged into another; request the surviving asset instead`
      ) as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const maxDepth = Math.min(query.max_depth ?? 8, 32);
    const mode = query.mode || 'traverse';
    let direction: 'outgoing' | 'incoming' | 'both' = query.direction || 'outgoing';
    let allowedTypes: Set<string> | null = null;

    if (mode === 'dependencies') {
      direction = 'outgoing';
      allowedTypes = new Set(['depends_on', 'installed_via', 'hosted_on']);
    } else if (mode === 'dependents' || mode === 'impact') {
      direction = 'incoming';
      allowedTypes = new Set([
        'depends_on',
        'installed_via',
        'hosted_on',
        'dependency_of',
        'installs',
        'hosts',
      ]);
    } else if (query.relation_types && query.relation_types.length > 0) {
      allowedTypes = new Set(query.relation_types);
    }

    const visited = new Set<string>([root.id]);
    const paths = new Map<string, RelationPathHopDto[]>();
    paths.set(root.id, []);

    let frontier: string[] = [root.id];
    const nodes: TraversalNodeDto[] = [];
    let stoppedAtBound = false;

    for (let depth = 1; depth <= maxDepth; depth++) {
      if (frontier.length === 0) break;
      const nextFrontier: string[] = [];

      for (const currentId of frontier) {
        const edges = await this.relationList(currentId);
        for (const edge of edges) {
          if (direction === 'outgoing' && !edge.outgoing) continue;
          if (direction === 'incoming' && edge.outgoing) continue;
          if (allowedTypes && !allowedTypes.has(edge.relation_type)) continue;

          const otherId = edge.other_asset_id;
          if (visited.has(otherId)) continue;

          const otherAsset = this.assets.find((a) => a.id === otherId);
          if (!otherAsset) continue;
          if (!query.include_archived && otherAsset.lifecycle === 'archived') continue;
          if (otherAsset.lifecycle === 'merged') continue;

          visited.add(otherId);
          const currentPath = paths.get(currentId) || [];
          const newPath: RelationPathHopDto[] = [
            ...currentPath,
            {
              from_asset_id: currentId,
              to_asset_id: otherId,
              relation_type: edge.relation_type,
            },
          ];
          paths.set(otherId, newPath);
          nextFrontier.push(otherId);

          nodes.push({
            asset: otherAsset,
            depth,
            path: newPath,
          });
        }
      }

      if (depth === maxDepth) {
        stoppedAtBound = true;
      }
      frontier = nextFrontier;
    }

    let truncated = false;
    if (stoppedAtBound && frontier.length > 0) {
      for (const lastId of frontier) {
        const edges = await this.relationList(lastId);
        for (const edge of edges) {
          if (direction === 'outgoing' && !edge.outgoing) continue;
          if (direction === 'incoming' && edge.outgoing) continue;
          if (allowedTypes && !allowedTypes.has(edge.relation_type)) continue;
          if (!visited.has(edge.other_asset_id)) {
            const nextAsset = this.assets.find((a) => a.id === edge.other_asset_id);
            if (nextAsset && (query.include_archived || nextAsset.lifecycle !== 'archived')) {
              truncated = true;
              break;
            }
          }
        }
        if (truncated) break;
      }
    }

    return {
      root,
      nodes,
      truncated,
    };
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
      revision: null,
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

    if (payload.context_asset_id) {
      const contextAsset = this.assets.find((a) => a.id === payload.context_asset_id);
      if (
        contextAsset &&
        payload.expected_context_revision !== undefined &&
        payload.expected_context_revision !== null
      ) {
        if (contextAsset.revision !== payload.expected_context_revision) {
          const err = new Error(
            `asset revision mismatch for context ${contextAsset.id}: expected ${payload.expected_context_revision}, found ${contextAsset.revision}`
          ) as Error & { category?: string };
          err.category = 'stale_revision';
          throw err;
        }
      }
      if (contextAsset) {
        const nextRev = (contextAsset.revision ?? 1) + 1;
        contextAsset.revision = nextRev;
        const cDetail = this.details.get(contextAsset.id);
        if (cDetail) cDetail.revision = nextRev;
      }
    }

    this.storedRelations.splice(idx, 1);
    return {
      operation: 'relation.remove',
      asset_ids: [],
      revision: null,
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

  async duplicateCandidates(query?: DuplicateQuery): Promise<Page<DuplicateCandidateDto>> {
    let eligible = [...this.assets];
    if (query?.include_archived === false) {
      eligible = eligible.filter((a) => a.lifecycle === 'active');
    } else {
      eligible = eligible.filter((a) => a.lifecycle === 'active' || a.lifecycle === 'archived');
    }

    if (query?.kinds && query.kinds.length > 0) {
      eligible = eligible.filter((a) => query.kinds!.includes(a.kind));
    }

    const candidates: DuplicateCandidateDto[] = [];
    for (let i = 0; i < eligible.length; i++) {
      for (let j = i + 1; j < eligible.length; j++) {
        const a = eligible[i];
        const b = eligible[j];
        if (a.kind !== b.kind) continue;

        const left = a.id < b.id ? a : b;
        const right = a.id < b.id ? b : a;

        const evidence: Array<Record<string, unknown>> = [];
        const evidence_labels: string[] = [];

        const normA = a.name.toLowerCase().trim();
        const normB = b.name.toLowerCase().trim();
        if (normA === normB) {
          evidence.push({
            evidence: 'same_normalized_name',
            normalized_name: normA,
            kind: a.kind,
          });
          evidence_labels.push(`same normalized name (${normA})`);
        }

        const detA = this.details.get(a.id);
        const detB = this.details.get(b.id);
        if (detA && detB) {
          if (a.kind.startsWith('software')) {
            const swA = detA.details as SoftwareRecordDto;
            const swB = detB.details as SoftwareRecordDto;
            if (swA?.install_location && swB?.install_location && swA.install_location === swB.install_location) {
              evidence.push({
                evidence: 'same_install_location',
                location: swA.install_location,
              });
              evidence_labels.push(`same install location (${swA.install_location})`);
            }
          } else if (a.kind.startsWith('service')) {
            const sA = detA.details as ServiceRecordDto;
            const sB = detB.details as ServiceRecordDto;
            if (sA?.provider && sB?.provider && sA.provider === sB.provider) {
              evidence.push({
                evidence: 'same_provider',
                provider: sA.provider,
              });
              evidence_labels.push(`same provider (${sA.provider})`);
            }
            if (sA?.domain_name && sB?.domain_name && sA.domain_name === sB.domain_name) {
              evidence.push({
                evidence: 'same_domain',
                domain: sA.domain_name,
              });
              evidence_labels.push(`same domain (${sA.domain_name})`);
            }
          }
        }

        if (evidence.length > 0) {
          candidates.push({
            left,
            right,
            evidence,
            evidence_labels,
          });
        }
      }
    }

    const total = candidates.length;
    const offset = query?.offset ?? 0;
    const limit = query?.limit ?? 20;
    const items = candidates.slice(offset, offset + limit);

    return {
      items,
      offset,
      limit,
      total,
    };
  }

  async mergePreview(winner_id: string, loser_id: string): Promise<MergePreviewDto> {
    if (winner_id === loser_id) {
      const err = new Error('cannot merge an asset into itself') as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const winner = this.assets.find((a) => a.id === winner_id);
    if (!winner) {
      const err = new Error(`winner asset not found: ${winner_id}`) as Error & { category?: string };
      err.category = 'not_found';
      throw err;
    }

    const loser = this.assets.find((a) => a.id === loser_id);
    if (!loser) {
      const err = new Error(`loser asset not found: ${loser_id}`) as Error & { category?: string };
      err.category = 'not_found';
      throw err;
    }

    const conflicts: string[] = [];
    const notes: string[] = [];

    if (loser.lifecycle === 'merged') {
      conflicts.push('Loser is already merged into another asset');
    }
    if (winner.lifecycle !== 'active') {
      conflicts.push('Winner must be active to receive a merge');
    }
    if (loser.kind !== winner.kind) {
      conflicts.push(`Cannot merge assets of different kinds: '${loser.kind}' vs '${winner.kind}'`);
    }

    const transferred_tags = (loser.tags || []).filter((t) => !(winner.tags || []).includes(t));

    const loser_relations = this.storedRelations.filter(
      (r) => r.source === loser_id || r.target === loser_id
    );
    const winner_relations = this.storedRelations.filter(
      (r) => r.source === winner_id || r.target === winner_id
    );

    let transferred_relations_count = 0;
    let redundant_relations_count = 0;

    for (const rel of loser_relations) {
      const other = rel.source === loser_id ? rel.target : rel.source;
      if (other === winner_id) {
        redundant_relations_count++;
        continue;
      }
      const norm = normalizeRelationFact(
        rel.source === loser_id ? winner_id : rel.source,
        rel.type,
        rel.target === loser_id ? winner_id : rel.target
      );
      const isDup = winner_relations.some((wr) => {
        const wNorm = normalizeRelationFact(wr.source, wr.type, wr.target);
        return wNorm.source === norm.source && wNorm.target === norm.target && wNorm.type === norm.type;
      });
      if (isDup) {
        redundant_relations_count++;
      } else {
        transferred_relations_count++;
      }
    }

    const detWinner = this.details.get(winner_id);
    const detLoser = this.details.get(loser_id);
    if (detWinner && detLoser && winner.kind.startsWith('service')) {
      const sW = detWinner.details as ServiceRecordDto;
      const sL = detLoser.details as ServiceRecordDto;
      const fieldConflicts: string[] = [];
      const checkF = (name: string, a?: string | null, b?: string | null) => {
        if (a && b && a !== b) {
          fieldConflicts.push(`${name}: '${a}' vs '${b}'`);
        }
      };
      checkF('plan', sW.plan, sL.plan);
      checkF('provider', sW.provider, sL.provider);
      checkF('domain_name', sW.domain_name, sL.domain_name);
      checkF('notes', sW.notes, sL.notes);
      if (sW.cost_minor != null && sL.cost_minor != null && sW.cost_minor !== sL.cost_minor) {
        fieldConflicts.push(`cost: ${sW.cost_minor} vs ${sL.cost_minor}`);
      }
      if (fieldConflicts.length > 0) {
        conflicts.push(`Service details conflict on: ${fieldConflicts.join(', ')}`);
      }
    }

    if (winner.kind.startsWith('media') && loser.kind.startsWith('media')) {
      notes.push("Winner's media metadata is kept. Loser's media record is preserved in the audit log.");
    }
    if (winner.kind.startsWith('software') && loser.kind.startsWith('software')) {
      notes.push("Winner's software metadata is kept. Loser's software record is preserved in the audit log.");
    }

    return {
      winner,
      loser,
      winner_revision: winner.revision ?? 1,
      loser_revision: loser.revision ?? 1,
      can_merge: conflicts.length === 0,
      conflicts,
      transferred_tags,
      transferred_external_refs: [],
      redundant_external_refs: [],
      transferred_relations_count,
      redundant_relations_count,
      notes,
    };
  }

  async mergeApply(command: MergeApplyCommand): Promise<MutationReceiptDto> {
    const preview = await this.mergePreview(command.winner_id, command.loser_id);
    if (!preview.can_merge) {
      const err = new Error(
        `cannot merge assets: ${preview.conflicts.join('; ')}`
      ) as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const winner = this.assets.find((a) => a.id === command.winner_id)!;
    const loser = this.assets.find((a) => a.id === command.loser_id)!;

    if (
      command.expected_winner_revision !== undefined &&
      command.expected_winner_revision !== null
    ) {
      if ((winner.revision ?? 1) !== command.expected_winner_revision) {
        const err = new Error(
          `asset revision mismatch for winner ${winner.id}: expected ${command.expected_winner_revision}, found ${winner.revision ?? 1}`
        ) as Error & { category?: string };
        err.category = 'stale_revision';
        throw err;
      }
    }
    if (
      command.expected_loser_revision !== undefined &&
      command.expected_loser_revision !== null
    ) {
      if ((loser.revision ?? 1) !== command.expected_loser_revision) {
        const err = new Error(
          `asset revision mismatch for loser ${loser.id}: expected ${command.expected_loser_revision}, found ${loser.revision ?? 1}`
        ) as Error & { category?: string };
        err.category = 'stale_revision';
        throw err;
      }
    }

    // Tombstone the loser
    loser.lifecycle = 'merged';
    loser.revision = (loser.revision ?? 1) + 1;
    const loserDetail = this.details.get(command.loser_id);
    if (loserDetail) {
      loserDetail.lifecycle = 'merged';
      loserDetail.merged_into = command.winner_id;
      loserDetail.details = {
        module: 'merged_redirect',
        surviving_asset_id: command.winner_id,
      };
    }

    // Merge tags into winner
    const mergedTags = Array.from(new Set([...(winner.tags || []), ...(loser.tags || [])]));
    winner.tags = mergedTags;
    const winnerDetail = this.details.get(command.winner_id);
    if (winnerDetail) {
      winnerDetail.tags = mergedTags;
      winnerDetail.revision++;
      winnerDetail.updated_at = new Date().toISOString();
    }
    winner.updated_at = new Date().toISOString();

    // Re-point loser relations
    for (const rel of this.storedRelations) {
      if (rel.source === command.loser_id) {
        if (rel.target === command.winner_id) continue;
        rel.source = command.winner_id;
      }
      if (rel.target === command.loser_id) {
        if (rel.source === command.winner_id) continue;
        rel.target = command.winner_id;
      }
    }

    this.recordActivity(
      'asset.merged',
      command.winner_id,
      winner.name,
      { loser_id: command.loser_id, loser_name: loser.name },
      'asset'
    );

    return {
      operation: 'asset.merge',
      asset_ids: [command.winner_id, command.loser_id],
      revision: winnerDetail ? winnerDetail.revision : null,
      changed: true,
      warnings: [],
    };
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
    if (!sourceDir || !sourceDir.trim()) {
      const err = new Error('Source directory cannot be empty') as Error & { category?: string };
      err.category = 'invalid_input';
      throw err;
    }
    this.importPreviewCalls.push(sourceDir);

    if (this.nextImportPreview) {
      return this.nextImportPreview;
    }

    if (sourceDir.includes('malformed')) {
      return {
        valid: false,
        source_dir: sourceDir,
        format: 'assetmesh-portable-export',
        version: 1,
        app_version: '0.3.0',
        created_at: new Date().toISOString(),
        record_counts: {},
        modules: [],
        dispositions: {
          assets_created: 0,
          assets_updated: 0,
          media_created: 0,
          media_updated: 0,
          software_created: 0,
          software_updated: 0,
          services_created: 0,
          services_updated: 0,
          relations_created: 0,
          relations_updated: 0,
          external_refs_created: 0,
          external_refs_deduplicated: 0,
          activity_created: 0,
          tags_created: 0,
        },
        errors: ['Failed to read bundle: invalid json syntax in assets.jsonl'],
        fingerprint: 'sha256-malformed-' + sourceDir,
      };
    }

    if (sourceDir.includes('unsupported')) {
      return {
        valid: false,
        source_dir: sourceDir,
        format: 'assetmesh-portable-export',
        version: 999,
        app_version: '99.0.0',
        created_at: new Date().toISOString(),
        record_counts: {},
        modules: [],
        dispositions: {
          assets_created: 0,
          assets_updated: 0,
          media_created: 0,
          media_updated: 0,
          software_created: 0,
          software_updated: 0,
          services_created: 0,
          services_updated: 0,
          relations_created: 0,
          relations_updated: 0,
          external_refs_created: 0,
          external_refs_deduplicated: 0,
          activity_created: 0,
          tags_created: 0,
        },
        errors: ['Unsupported export version 999, supported version is 1'],
        fingerprint: 'sha256-unsupported-' + sourceDir,
      };
    }

    if (sourceDir.includes('collision')) {
      return {
        valid: false,
        source_dir: sourceDir,
        format: 'assetmesh-portable-export',
        version: 1,
        app_version: '0.3.0',
        created_at: new Date().toISOString(),
        record_counts: { assets: 1, external_refs: 1 },
        modules: ['media'],
        dispositions: {
          assets_created: 0,
          assets_updated: 0,
          media_created: 0,
          media_updated: 0,
          software_created: 0,
          software_updated: 0,
          services_created: 0,
          services_updated: 0,
          relations_created: 0,
          relations_updated: 0,
          external_refs_created: 0,
          external_refs_deduplicated: 0,
          activity_created: 0,
          tags_created: 0,
        },
        errors: ['Collision: external ref imdb:tt0000001 is already attached to another asset'],
        fingerprint: 'sha256-collision-' + sourceDir,
      };
    }

    return {
      valid: true,
      source_dir: sourceDir,
      format: 'assetmesh-portable-export',
      version: 1,
      app_version: '0.3.0',
      created_at: '2026-01-01T00:00:00Z',
      record_counts: {
        assets: 3,
        media: 1,
        software: 1,
        services: 1,
        relations: 1,
      },
      modules: ['media', 'software', 'services'],
      dispositions: {
        assets_created: 3,
        assets_updated: 0,
        media_created: 1,
        media_updated: 0,
        software_created: 1,
        software_updated: 0,
        services_created: 1,
        services_updated: 0,
        relations_created: 1,
        relations_updated: 0,
        external_refs_created: 2,
        external_refs_deduplicated: 0,
        activity_created: 3,
        tags_created: 2,
      },
      fingerprint: 'sha256-valid-' + sourceDir,
      errors: [],
    };
  }

  async portableImportApply(sourceDir: string, expectedFingerprint?: string): Promise<ImportReceipt> {
    if (!sourceDir || !sourceDir.trim()) {
      const err = new Error('Source directory cannot be empty') as Error & { category?: string };
      err.category = 'invalid_input';
      throw err;
    }
    if (expectedFingerprint !== undefined && !expectedFingerprint.trim()) {
      const err = new Error('Expected bundle fingerprint cannot be empty') as Error & { category?: string };
      err.category = 'invalid_input';
      throw err;
    }
    this.importApplyCalls.push(sourceDir);

    if (this.nextImportReceipt) {
      return this.nextImportReceipt;
    }

    if (expectedFingerprint && (expectedFingerprint.includes('mismatch') || expectedFingerprint === 'mismatch')) {
      const err = new Error('Bundle content changed since preview') as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    if (sourceDir.includes('malformed') || sourceDir.includes('unsupported')) {
      const err = new Error('Bundle validation failed') as Error & { category?: string };
      err.category = 'invalid_input';
      throw err;
    }

    if (sourceDir.includes('collision')) {
      const err = new Error('Import collision: external ref conflict') as Error & { category?: string };
      err.category = 'conflict';
      throw err;
    }

    const preview = await this.portableImportPreview(sourceDir);
    if (!preview.valid) {
      const err = new Error(preview.errors[0] || 'Import preflight rejected') as Error & {
        category?: string;
      };
      err.category = 'invalid_input';
      throw err;
    }

    return {
      success: true,
      source_dir: sourceDir,
      applied_at: new Date().toISOString(),
      report: preview.dispositions,
    };
  }

  async getAppSettings(): Promise<AppSettings> {
    return {
      db_path: '/Users/diaoyuxuan/Library/Application Support/com.assetmesh.desktop/assetmesh.db',
      db_status: 'Ready',
      app_version: '0.3.0',
      providers: [
        {
          name: 'macos_applications',
          display_name: 'macOS Applications',
          available: true,
          details: '/Applications, ~/Applications',
        },
        {
          name: 'homebrew',
          display_name: 'Homebrew',
          available: true,
          details: 'Homebrew 4.4.0 in PATH',
        },
        {
          name: 'cli_tools',
          display_name: 'CLI Tools (npm, pipx)',
          available: true,
          details: 'npm and pipx detected in PATH',
        },
      ],
      capabilities: this.capabilities,
    };
  }
}

