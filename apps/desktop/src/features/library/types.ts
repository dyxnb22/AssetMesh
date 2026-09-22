export interface AssetSummary {
  id: string;
  kind: string;
  name: string;
  lifecycle: 'active' | 'archived' | 'merged';
  subtitle: string | null;
  tags: string[];
  updated_at: string;
}

export interface Page<T> {
  items: T[];
  offset: number;
  limit: number;
  total: number | null;
}

export interface LibraryQuery {
  lifecycle?: 'active' | 'archived' | 'active_or_archived' | 'all';
  modules?: string[];
  kinds?: string[];
  tags?: string[];
  sort?: 'updated_desc' | 'updated_asc' | 'name_asc' | 'name_desc' | 'kind_asc';
  limit?: number;
  offset?: number;
}

export interface AppCapabilities {
  version: string;
  modules: string[];
  asset_kinds: string[];
  relation_types: string[];
  storable_relation_types: string[];
  features: {
    runtime_enrichment: boolean;
    projects: boolean;
    agent_capabilities: boolean;
    knowledge_collections: boolean;
  };
}

export type AppStatus =
  | { status: 'loading' }
  | { status: 'ready'; db_path: string }
  | { status: 'setup_failure'; message: string }
  | { status: 'corrupt_failure'; message: string };

export interface DesktopError {
  category: string;
  message: string;
}

export type ActiveModule = 'all' | 'media' | 'software' | 'services';
export type ActiveSection = 'library' | 'relations' | 'activity';
export type SortOption = 'updated_desc' | 'updated_asc' | 'name_asc' | 'name_desc' | 'kind_asc';
export type LifecycleOption = 'active' | 'active_or_archived' | 'all';

export interface NavigationState {
  section: ActiveSection;
  module: ActiveModule;
  lifecycle: LifecycleOption;
  sort: SortOption;
  kind: string | null;
  tag: string | null;
  page: number; // 1-indexed
  pageSize: number;
  selectedAssetId: string | null;
}
