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
  Page,
  ServiceCommand,
  ServiceRecordDto,
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
      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        lifecycle: 'archived',
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
        revision: null,
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
      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        lifecycle: 'archived',
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
        revision: null,
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
      const updatedDetail: AssetDetailDto = {
        ...currentDetail,
        lifecycle: 'archived',
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
        revision: null,
        changed: true,
        warnings: [],
      };
    }

    throw new Error('Unsupported service command action');
  }
}
