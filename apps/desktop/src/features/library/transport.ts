import { invoke } from '@tauri-apps/api/core';
import type {
  AppCapabilities,
  AppStatus,
  AssetDetailDto,
  AssetSummary,
  LibraryQuery,
  LibrarySearchQuery,
  Page,
} from './types';

export interface DesktopTransport {
  getCapabilities(): Promise<AppCapabilities>;
  getStatus(): Promise<AppStatus>;
  init(dbPath: string): Promise<AppStatus>;
  listAssets(query?: LibraryQuery): Promise<Page<AssetSummary>>;
  searchAssets(query: LibrarySearchQuery): Promise<Page<AssetSummary>>;
  getAsset(id: string): Promise<AssetDetailDto>;
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
}

let activeTransport: DesktopTransport = new TauriTransport();

export function getTransport(): DesktopTransport {
  return activeTransport;
}

export function setTransport(transport: DesktopTransport): void {
  activeTransport = transport;
}
