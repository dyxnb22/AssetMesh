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

export interface ExternalRefDto {
  namespace: string;
  external_id: string;
  source_url: string | null;
}

export interface MediaRecordDto {
  module: 'media';
  asset_id: string;
  media_type: string;
  status: string;
  rating?: number | null;
  year?: number | null;
  platform?: string | null;
  progress?: {
    unit?: string;
    current?: number;
    total?: number | null;
  } | null;
  notes?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
}

export interface SoftwareRecordDto {
  module: 'software';
  asset_id: string;
  category: string;
  install_source?: unknown;
  version?: string | null;
  install_location?: string | null;
  executable_path?: string | null;
  purpose?: string | null;
  notes?: string | null;
  discovered_at?: string | null;
  installed_at?: string | null;
  architecture?: string | null;
}

export interface ServiceRecordDto {
  module: 'services';
  asset_id: string;
  service_type: string;
  provider?: string | null;
  account_label?: string | null;
  endpoint_url?: string | null;
  dashboard_url?: string | null;
  domain_name?: string | null;
  plan?: string | null;
  cost_minor?: number | null;
  currency?: string | null;
  billing_cadence?: string | null;
  renews_at?: string | null;
  expires_at?: string | null;
  auto_renew?: boolean | null;
  notes?: string | null;
}

export interface MergedRedirectDto {
  module: 'merged_redirect';
  surviving_asset_id: string;
}

export interface UnknownDetailsDto {
  module: 'unknown';
  [key: string]: unknown;
}

export type AssetDetailsDto =
  | MediaRecordDto
  | SoftwareRecordDto
  | ServiceRecordDto
  | MergedRedirectDto
  | UnknownDetailsDto;

export interface AssetDetailDto {
  id: string;
  kind: string;
  name: string;
  summary: string | null;
  lifecycle: 'active' | 'archived' | 'merged';
  revision: number;
  created_at: string;
  updated_at: string;
  archived_at: string | null;
  merged_into: string | null;
  details: AssetDetailsDto;
  tags: string[];
  external_refs: ExternalRefDto[];
}

