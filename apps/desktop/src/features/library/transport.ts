import { invoke } from '@tauri-apps/api/core';
import type {
  AppCapabilities,
  AppStatus,
  AssetDetailDto,
  AssetSummary,
  ClassifiedCandidateDto,
  DesktopError,
  LibraryQuery,
  LibrarySearchQuery,
  MediaCommand,
  MutationReceiptDto,
  NeighborViewDto,
  Page,
  RelationAttachPayload,
  RelationNeighborsQuery,
  RelationRemovePayload,
  RelationTraverseQuery,
  RelationViewDto,
  ServiceCommand,
  SoftwareCommand,
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
} from './types';

export interface DesktopTransport {
  getCapabilities(): Promise<AppCapabilities>;
  getStatus(): Promise<AppStatus>;
  init(dbPath: string): Promise<AppStatus>;
  listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>>;
  searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>>;
  getAsset(id: string): Promise<AssetDetailDto>;
  softwareDiscover(): Promise<ClassifiedCandidateDto[]>;
  softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto>;
  mediaCommand(command: MediaCommand): Promise<MutationReceiptDto>;
  serviceCommand(command: ServiceCommand): Promise<MutationReceiptDto>;
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
  portableExport(targetDir: string): Promise<ExportReceipt>;
  portableImportPreview(sourceDir: string): Promise<ImportPreview>;
  portableImportApply(sourceDir: string): Promise<ImportReceipt>;
  getAppSettings(): Promise<AppSettings>;
}

export class TauriTransport implements DesktopTransport {
  async getCapabilities(): Promise<AppCapabilities> {
    return await invoke<AppCapabilities>('app_capabilities');
  }

  async getStatus(): Promise<AppStatus> {
    return await invoke<AppStatus>('app_status');
  }

  async init(dbPath: string): Promise<AppStatus> {
    return await invoke<AppStatus>('app_init', { dbPath });
  }

  async listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>> {
    return await invoke<Page<AssetSummary>>('library_list', { query });
  }

  async searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>> {
    return await invoke<Page<AssetSummary>>('library_search', { query });
  }

  async getAsset(id: string): Promise<AssetDetailDto> {
    return await invoke<AssetDetailDto>('library_get', { id });
  }

  async softwareDiscover(): Promise<ClassifiedCandidateDto[]> {
    return await invoke<ClassifiedCandidateDto[]>('software_discover');
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
    return await invoke<Page<DuplicateCandidateDto>>('duplicate_candidates', { query: query || {} });
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

  async portableExport(targetDir: string): Promise<ExportReceipt> {
    return await invoke<ExportReceipt>('portable_export', { targetDir });
  }

  async portableImportPreview(sourceDir: string): Promise<ImportPreview> {
    return await invoke<ImportPreview>('portable_import_preview', { sourceDir });
  }

  async portableImportApply(sourceDir: string): Promise<ImportReceipt> {
    return await invoke<ImportReceipt>('portable_import_apply', { sourceDir });
  }

  async getAppSettings(): Promise<AppSettings> {
    return await invoke<AppSettings>('app_settings');
  }
}

export function normalizeDesktopError(err: unknown): DesktopError {
  if (typeof err === 'object' && err !== null) {
    const obj = err as Record<string, unknown>;
    const category = typeof obj.category === 'string' ? obj.category : 'internal_error';
    const message = typeof obj.message === 'string' ? obj.message : String(err);
    return { category, message };
  }
  return {
    category: 'internal_error',
    message: String(err),
  };
}

let activeTransport: DesktopTransport = new TauriTransport();

export function getTransport(): DesktopTransport {
  return activeTransport;
}

export function setTransport(transport: DesktopTransport): void {
  activeTransport = transport;
}
