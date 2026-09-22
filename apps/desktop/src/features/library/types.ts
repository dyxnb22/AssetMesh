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

export interface LibrarySearchQuery {
  text: string;
  lifecycle?: 'active' | 'archived' | 'active_or_archived' | 'all';
  modules?: string[];
  kinds?: string[];
  tags?: string[];
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
  search: string;
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

export interface MutationReceiptDto {
  operation: string;
  asset_ids: string[];
  revision: number | null;
  changed: boolean;
  warnings: string[];
}

export interface CandidateRefDto {
  namespace: string;
  external_id: string;
}

export interface SoftwareCandidateDto {
  provider: string;
  display_name: string;
  category: string;
  install_source: string;
  version?: string | null;
  install_location?: string | null;
  executable_path?: string | null;
  external_refs: CandidateRefDto[];
  metadata?: unknown;
}

export interface ClassifiedCandidateDto {
  candidate: SoftwareCandidateDto;
  disposition: string;
  matched_asset_ids: string[];
  message?: string | null;
}

export type SoftwareCommand =
  | {
      action: 'create';
      name: string;
      category: string;
      summary?: string | null;
      install_source?: string | null;
      version?: string | null;
      install_location?: string | null;
      executable_path?: string | null;
      purpose?: string | null;
      notes?: string | null;
      architecture?: string | null;
      tags?: string[];
    }
  | {
      action: 'update_metadata';
      asset_id: string;
      expected_revision?: number | null;
      name?: string | null;
      summary?: string | null;
      version?: string | null;
      install_location?: string | null;
      executable_path?: string | null;
      purpose?: string | null;
      notes?: string | null;
      architecture?: string | null;
    }
  | {
      action: 'adopt_candidate';
      candidate: SoftwareCandidateDto;
      target?: string | null;
      purpose?: string | null;
      notes?: string | null;
      tags?: string[];
    }
  | {
      action: 'archive';
      asset_id: string;
    };

export type MediaCommand =
  | {
      action: 'create';
      title: string;
      media_type: string;
      summary?: string | null;
      status?: string | null;
      rating?: number | null;
      year?: number | null;
      platform?: string | null;
      progress_unit?: string | null;
      progress_current?: number | null;
      progress_total?: number | null;
      notes?: string | null;
      tags?: string[];
    }
  | {
      action: 'update_metadata';
      asset_id: string;
      expected_revision?: number | null;
      title?: string | null;
      summary?: string | null;
      year?: number | null;
      platform?: string | null;
      notes?: string | null;
    }
  | {
      action: 'transition_status';
      asset_id: string;
      status: string;
    }
  | {
      action: 'update_progress';
      asset_id: string;
      unit?: string | null;
      current?: number | null;
      total?: number | null;
    }
  | {
      action: 'rate';
      asset_id: string;
      rating: number;
    }
  | {
      action: 'archive';
      asset_id: string;
    };

export type ServiceCommand =
  | {
      action: 'create';
      name: string;
      service_type: string;
      summary?: string | null;
      provider?: string | null;
      account_label?: string | null;
      endpoint_url?: string | null;
      dashboard_url?: string | null;
      domain_name?: string | null;
      plan?: string | null;
      cost?: string | null;
      currency?: string | null;
      billing_cadence?: string | null;
      renews_at?: string | null;
      expires_at?: string | null;
      auto_renew?: boolean | null;
      notes?: string | null;
      tags?: string[];
    }
  | {
      action: 'update';
      asset_id: string;
      expected_revision?: number | null;
      name?: string | null;
      summary?: string | null;
      provider?: string | null;
      account_label?: string | null;
      endpoint_url?: string | null;
      dashboard_url?: string | null;
      domain_name?: string | null;
      plan?: string | null;
      cost?: string | null;
      currency?: string | null;
      billing_cadence?: string | null;
      renews_at?: string | null;
      expires_at?: string | null;
      auto_renew?: boolean | null;
      notes?: string | null;
    }
  | {
      action: 'record_renewal';
      asset_id: string;
      renews_at: string;
      cost?: string | null;
      currency?: string | null;
      next_renews_at?: string | null;
      next_expires_at?: string | null;
    }
  | {
      action: 'archive';
      asset_id: string;
    };


