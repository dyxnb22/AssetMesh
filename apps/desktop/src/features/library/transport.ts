import { invoke } from '@tauri-apps/api/core';
import { decodeAssetDetail, decodeAssetSummary, decodeDuplicateCandidate, decodePage } from './wire';
import type {
  AppCapabilities,
  AppStatus,
  AssetDetailDto,
  AssetSummary,
  DesktopError,
  DesktopErrorCategory,
  LibraryQuery,
  LibrarySearchQuery,
  MediaCommand,
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
  SoftwareCommand,
  ServiceRuntimeStatusDto,
  ServiceRuntimeLogsDto,
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
} from './types';

export type StartupStage = 'frontend_loaded' | 'shell_visible' | 'workspace_mounted' | 'first_list_ready';

export interface DesktopTransport {
  startupTiming(stage: StartupStage, webMs: number): Promise<void>;
  getCapabilities(): Promise<AppCapabilities>;
  getStatus(): Promise<AppStatus>;
  init(dbPath?: string): Promise<AppStatus>;
  listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>>;
  searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>>;
  mediaStatusCounts(query?: LibraryQuery): Promise<MediaStatusCountDto[]>;
  getAsset(id: string): Promise<AssetDetailDto>;
  softwareDiscover(): Promise<DiscoveryReport>;
  softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto>;
  mediaCommand(command: MediaCommand): Promise<MutationReceiptDto>;
  serviceCommand(command: ServiceCommand): Promise<MutationReceiptDto>;
  /** Starts the saved command of a local service; the backend owns the process. */
  serviceRuntimeStart(assetId: string): Promise<ServiceRuntimeStatusDto>;
  /** Stops the process AssetMesh started; returns null when none is held. */
  serviceRuntimeStop(assetId: string): Promise<ServiceRuntimeStatusDto | null>;
  serviceRuntimeRestart(assetId: string): Promise<ServiceRuntimeStatusDto>;
  serviceRuntimeStatus(assetId: string): Promise<ServiceRuntimeStatusDto>;
  serviceRuntimeStatuses(forceRefresh?: boolean): Promise<ServiceRuntimeStatusDto[]>;
  serviceRuntimeLogs(assetId: string, since?: number, runId?: string): Promise<ServiceRuntimeLogsDto>;
  serviceOpenPage(assetId: string): Promise<void>;
  infoCommand(command: InfoCommand): Promise<MutationReceiptDto>;
  relationList(assetId: string): Promise<RelationViewDto[]>;
  relationNeighbors(query: RelationNeighborsQuery): Promise<NeighborViewDto[]>;
  relationTraverse(query: RelationTraverseQuery): Promise<TraversalViewDto>;
  relationAttach(payload: RelationAttachPayload): Promise<MutationReceiptDto>;
  relationRemove(payload: RelationRemovePayload): Promise<MutationReceiptDto>;
  activityQuery(query?: ActivityQuery): Promise<Page<ActivityViewDto>>;
  duplicateCandidates(query?: DuplicateQuery): Promise<Page<DuplicateCandidateDto>>;
  mergePreview(winner_id: string, loser_id: string): Promise<MergePreviewDto>;
  mergeApply(command: MergeApplyCommand): Promise<MutationReceiptDto>;
  pickDirectory(prompt?: string): Promise<string | null>;
  portableExport(targetDir: string, includeApiKeys?: boolean): Promise<ExportReceipt>;
  portableImportPreview(sourceDir: string): Promise<ImportPreview>;
  portableImportApply(sourceDir: string, expectedFingerprint: string): Promise<ImportReceipt>;
  backupStatus(): Promise<BackupStatus>;
  backupCreate(): Promise<BackupEntry>;
  backupPreview(sourceDir: string): Promise<BackupEntry>;
  backupRestore(sourceDir: string, expectedFingerprint: string): Promise<RestoreReceipt>;
  backupPreferences(): Promise<Record<string, string>>;
  backupSavePreferences(preferences: Record<string, string>): Promise<void>;
  backupTick(): Promise<void>;
  backupExportCopy(targetDir: string): Promise<string>;
}

export class TauriTransport implements DesktopTransport {
  async startupTiming(stage: StartupStage, webMs: number): Promise<void> {
    await invoke<void>('app_startup_timing', { stage, webMs });
  }
  async getCapabilities(): Promise<AppCapabilities> {
    return await invoke<AppCapabilities>('app_capabilities');
  }

  async getStatus(): Promise<AppStatus> {
    return await invoke<AppStatus>('app_status');
  }

  async init(dbPath?: string): Promise<AppStatus> {
    return await invoke<AppStatus>('app_init', { dbPath });
  }

  async listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>> {
    return decodePage(await invoke<unknown>('library_list', { query }), decodeAssetSummary);
  }

  async searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>> {
    return decodePage(await invoke<unknown>('library_search', { query }), decodeAssetSummary);
  }

  async mediaStatusCounts(query?: LibraryQuery): Promise<MediaStatusCountDto[]> {
    return await invoke<MediaStatusCountDto[]>('library_media_status_counts', {
      query: query ?? {},
    });
  }

  async getAsset(id: string): Promise<AssetDetailDto> {
    return decodeAssetDetail(await invoke<unknown>('library_get', { id }));
  }

  async softwareDiscover(): Promise<DiscoveryReport> {
    return await invoke<DiscoveryReport>('software_discover');
  }

  async softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('software_command', { command });
  }

  async mediaCommand(command: MediaCommand): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('media_command', { command });
  }

  async serviceCommand(command: ServiceCommand): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('service_command', { command });
  }

  async serviceRuntimeStart(assetId: string): Promise<ServiceRuntimeStatusDto> {
    return await invoke<ServiceRuntimeStatusDto>('service_runtime_start', { assetId });
  }

  async serviceRuntimeStop(assetId: string): Promise<ServiceRuntimeStatusDto | null> {
    return await invoke<ServiceRuntimeStatusDto | null>('service_runtime_stop', { assetId });
  }

  async serviceRuntimeRestart(assetId: string): Promise<ServiceRuntimeStatusDto> {
    return await invoke<ServiceRuntimeStatusDto>('service_runtime_restart', { assetId });
  }

  async serviceRuntimeStatus(assetId: string): Promise<ServiceRuntimeStatusDto> {
    return await invoke<ServiceRuntimeStatusDto>('service_runtime_status', { assetId });
  }

  async serviceRuntimeStatuses(forceRefresh?: boolean): Promise<ServiceRuntimeStatusDto[]> {
    return await invoke<ServiceRuntimeStatusDto[]>('service_runtime_statuses', { forceRefresh: forceRefresh ?? false });
  }

  async serviceRuntimeLogs(assetId: string, since?: number, runId?: string): Promise<ServiceRuntimeLogsDto> {
    return await invoke<ServiceRuntimeLogsDto>('service_runtime_logs', {
      assetId,
      since: since ?? 0,
      runId: runId ?? null,
    });
  }

  async serviceOpenPage(assetId: string): Promise<void> {
    await invoke('service_open_page', { assetId });
  }

  async infoCommand(command: InfoCommand): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('info_command', { command });
  }

  async relationList(assetId: string): Promise<RelationViewDto[]> {
    return await invoke<RelationViewDto[]>('relation_list', { assetId });
  }

  async relationNeighbors(query: RelationNeighborsQuery): Promise<NeighborViewDto[]> {
    return await invoke<NeighborViewDto[]>('relation_neighbors', { query });
  }

  async relationTraverse(query: RelationTraverseQuery): Promise<TraversalViewDto> {
    return await invoke<TraversalViewDto>('relation_traverse', { query });
  }

  async relationAttach(payload: RelationAttachPayload): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('relation_attach', { payload });
  }

  async relationRemove(payload: RelationRemovePayload): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('relation_remove', { payload });
  }

  async activityQuery(query?: ActivityQuery): Promise<Page<ActivityViewDto>> {
    return await invoke<Page<ActivityViewDto>>('activity_query', { query: query || {} });
  }

  async duplicateCandidates(query?: DuplicateQuery): Promise<Page<DuplicateCandidateDto>> {
    return decodePage(await invoke<unknown>('duplicate_candidates', { query: query || {} }), decodeDuplicateCandidate);
  }

  async mergePreview(winner_id: string, loser_id: string): Promise<MergePreviewDto> {
    return await invoke<MergePreviewDto>('merge_preview', { query: { winner_id, loser_id } });
  }

  async mergeApply(command: MergeApplyCommand): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('merge_apply', { input: command });
  }

  async pickDirectory(prompt?: string): Promise<string | null> {
    return await invoke<string | null>('pick_directory', { prompt });
  }

  async portableExport(targetDir: string, includeApiKeys?: boolean): Promise<ExportReceipt> {
    return await invoke<ExportReceipt>('portable_export', { targetDir, includeApiKeys: includeApiKeys ?? false });
  }

  async portableImportPreview(sourceDir: string): Promise<ImportPreview> {
    return await invoke<ImportPreview>('portable_import_preview', { sourceDir });
  }

  async portableImportApply(sourceDir: string, expectedFingerprint: string): Promise<ImportReceipt> {
    return await invoke<ImportReceipt>('portable_import_apply', { sourceDir, expectedFingerprint });
  }

  async backupStatus(): Promise<BackupStatus> {
    return await invoke<BackupStatus>('backup_status');
  }
  async backupCreate(): Promise<BackupEntry> {
    return await invoke<BackupEntry>('backup_create');
  }
  async backupPreview(sourceDir: string): Promise<BackupEntry> {
    return await invoke<BackupEntry>('backup_preview', { sourceDir });
  }
  async backupRestore(sourceDir: string, expectedFingerprint: string): Promise<RestoreReceipt> {
    return await invoke<RestoreReceipt>('backup_restore', { sourceDir, expectedFingerprint });
  }
  async backupPreferences(): Promise<Record<string, string>> {
    return await invoke<Record<string, string>>('backup_preferences');
  }
  async backupSavePreferences(preferences: Record<string, string>): Promise<void> {
    return await invoke<void>('backup_save_preferences', { preferences });
  }
  async backupExportCopy(targetDir: string): Promise<string> {
    return await invoke<string>('backup_export_copy', { targetDir });
  }
  async backupTick(): Promise<void> {
    return await invoke<void>('backup_tick');
  }


}

export function normalizeDesktopError(err: unknown): DesktopError {
  if (typeof err === 'object' && err !== null) {
    const obj = err as Record<string, unknown>;
    if (isDesktopErrorCategory(obj.category) && typeof obj.message === 'string') {
      return { category: obj.category, message: obj.message };
    }
  }
  return {
    category: 'internal',
    message: 'An unexpected application error occurred.',
  };
}

const desktopErrorCategories: ReadonlySet<string> = new Set<DesktopErrorCategory>([
  'invalid_input', 'not_found', 'conflict', 'stale_revision', 'setup_required',
  'unavailable', 'permission_denied', 'unsupported', 'timeout', 'rate_limited',
  'corrupt_data', 'internal',
]);

function isDesktopErrorCategory(value: unknown): value is DesktopErrorCategory {
  return typeof value === 'string' && desktopErrorCategories.has(value);
}

let activeTransport: DesktopTransport = new TauriTransport();

export function getTransport(): DesktopTransport {
  return activeTransport;
}

export function setTransport(transport: DesktopTransport): void {
  activeTransport = transport;
}
