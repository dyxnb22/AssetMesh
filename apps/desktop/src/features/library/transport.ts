import { invoke } from '@tauri-apps/api/core';
import type {
  AppCapabilities,
  AppStatus,
  AssetDetailDto,
  AssetSummary,
  DesktopError,
  LibraryQuery,
  LibrarySearchQuery,
  MutationReceiptDto,
  Page,
  SoftwareCommand,
} from './types';

export interface DesktopTransport {
  getCapabilities(): Promise<AppCapabilities>;
  getStatus(): Promise<AppStatus>;
  init(dbPath: string): Promise<AppStatus>;
  listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>>;
  searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>>;
  getAsset(id: string): Promise<AssetDetailDto>;
  softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto>;
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

  async softwareCommand(command: SoftwareCommand): Promise<MutationReceiptDto> {
    return await invoke<MutationReceiptDto>('software_command', { command });
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
