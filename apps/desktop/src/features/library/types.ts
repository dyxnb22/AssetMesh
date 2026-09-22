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

export type DesktopErrorCategory =
  | 'invalid_input'
  | 'not_found'
  | 'conflict'
  | 'stale_revision'
  | 'setup_required'
  | 'storage_busy'
  | 'unavailable'
  | 'permission_denied'
  | 'unsupported'
  | 'corrupt_data'
  | 'internal';

export interface DesktopError {
  category: DesktopErrorCategory | string;
  message: string;
}

export type ActiveModule = 'all' | 'media' | 'software' | 'services';
export type ActiveSection =
  | 'library'
  | 'relations'
  | 'activity'
  | 'duplicates'
  | 'import-export'
  | 'settings';
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

export interface RelationViewDto {
  relation_id: string;
  other_asset_id: string;
  other_asset_name: string;
  relation_type: string;
  outgoing: boolean;
  note?: string | null;
  provenance: string;
  created_at: string;
}

export interface NeighborViewDto {
  asset: AssetSummary;
  edge: RelationViewDto;
}

export interface RelationPathHopDto {
  from_asset_id: string;
  to_asset_id: string;
  relation_type: string;
}

export interface TraversalNodeDto {
  asset: AssetSummary;
  depth: number;
  path: RelationPathHopDto[];
}

export interface TraversalViewDto {
  root: AssetSummary;
  nodes: TraversalNodeDto[];
  truncated: boolean;
}

export interface RelationNeighborsQuery {
  asset_id: string;
  direction?: 'outgoing' | 'incoming' | 'both';
  relation_types?: string[];
  include_archived?: boolean;
}

export interface RelationTraverseQuery {
  asset_id: string;
  mode?: 'dependencies' | 'dependents' | 'impact' | 'traverse';
  direction?: 'outgoing' | 'incoming' | 'both';
  relation_types?: string[];
  max_depth?: number;
  include_archived?: boolean;
}

export interface RelationAttachPayload {
  source_asset_id: string;
  relation_type: string;
  target_asset_id: string;
  note?: string;
}

export interface RelationRemovePayload {
  relation_id: string;
}

// =========================================================================
// Activity Types (P5-08)
// =========================================================================

export interface ActivityViewDto {
  id: string;
  event_type: string;
  module: string | null;
  occurred_at: string;
  actor: string;
  asset_id: string | null;
  asset_name: string | null;
  payload: Record<string, unknown>;
}

export interface ActivityQuery {
  asset_id?: string;
  event_types?: string[];
  modules?: string[];
  /**
   * Only events about assets of these kinds, e.g. `media.anime`. Narrower than
   * `modules`: Media owns several kinds, so "movie events only" is not a module
   * question.
   */
  kinds?: string[];
  actors?: string[];
  since?: string;
  until?: string;
  limit?: number;
  offset?: number;
}

// =========================================================================
// Duplicate Review & Merge Types (P5-08)
// =========================================================================

export interface DuplicateCandidateDto {
  left: AssetSummary;
  right: AssetSummary;
  evidence: Array<Record<string, unknown>>;
  evidence_labels: string[];
}

export interface DuplicateQuery {
  kinds?: string[];
  include_archived?: boolean;
  limit?: number;
  offset?: number;
}

export interface MergePreviewDto {
  winner: AssetSummary;
  loser: AssetSummary;
  can_merge: boolean;
  conflicts: string[];
  transferred_tags: string[];
  transferred_external_refs: ExternalRefDto[];
  redundant_external_refs: ExternalRefDto[];
  transferred_relations_count: number;
  redundant_relations_count: number;
  notes: string[];
}

export interface MergeApplyCommand {
  winner_id: string;
  loser_id: string;
}

// =========================================================================
// Import / Export & Settings Types (P5-09)
// =========================================================================

export type ThemePreference = 'system' | 'light' | 'dark';

export interface ExportReceipt {
  target_dir: string;
  format: string;
  version: number;
  app_version: string;
  created_at: string;
  record_counts: Record<string, number>;
  files: string[];
}

export interface ImportReport {
  assets_created: number;
  assets_updated: number;
  media_created: number;
  media_updated: number;
  software_created: number;
  software_updated: number;
  services_created: number;
  services_updated: number;
  relations_created: number;
  relations_updated: number;
  external_refs_created: number;
  external_refs_deduplicated: number;
  activity_created: number;
  tags_created: number;
}

export interface ImportPreview {
  valid: boolean;
  source_dir: string;
  format: string;
  version: number;
  app_version: string;
  created_at: string;
  record_counts: Record<string, number>;
  modules: string[];
  dispositions: ImportReport;
  fingerprint: string;
  errors: string[];
}

export interface ImportReceipt {
  success: boolean;
  source_dir: string;
  applied_at: string;
  report: ImportReport;
}

export interface ProviderStatus {
  name: string;
  display_name: string;
  available: boolean;
  details: string | null;
}

export interface AppSettings {
  db_path: string | null;
  db_status: string;
  app_version: string;
  providers: ProviderStatus[];
  capabilities: AppCapabilities;
}

