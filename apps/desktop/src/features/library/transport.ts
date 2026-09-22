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
