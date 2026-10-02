import type { AssetSummary, ImportPreview, ImportReport, MergePreviewDto } from '../features/library/types';

// Canned adapter responses. Business decisions are covered by the Rust/SQLite
// contracts; frontend tests choose the response they need to present.
export const EMPTY_IMPORT_REPORT: ImportReport = {
  assets_created: 0, assets_updated: 0,
  media_created: 0, media_updated: 0,
  software_created: 0, software_updated: 0,
  services_created: 0, services_updated: 0,
  info_created: 0, info_updated: 0,
  relations_created: 0, relations_updated: 0,
  external_refs_created: 0, external_refs_deduplicated: 0,
  activity_created: 0, tags_created: 0,
};

export function importPreviewFixture(sourceDir: string, overrides: Partial<ImportPreview> = {}): ImportPreview {
  return {
    valid: true,
    source_dir: sourceDir,
    format: 'assetmesh-portable-export',
    version: 1,
    app_version: '0.3.0',
    created_at: '2026-01-01T00:00:00Z',
    record_counts: { assets: 3, media: 1, software: 1, services: 1, relations: 1 },
    modules: ['media', 'software', 'services'],
    dispositions: {
      ...EMPTY_IMPORT_REPORT,
      assets_created: 3, media_created: 1, software_created: 1, services_created: 1,
      relations_created: 1, external_refs_created: 2, activity_created: 3, tags_created: 2,
    },
    fingerprint: `fixture-fingerprint:${sourceDir}`,
    errors: [],
    ...overrides,
  };
}

export function mergePreviewFixture(winner: AssetSummary, loser: AssetSummary, overrides: Partial<MergePreviewDto> = {}): MergePreviewDto {
  return {
    winner, loser,
    winner_revision: winner.revision ?? 1,
    loser_revision: loser.revision ?? 1,
    can_merge: true,
    conflicts: [],
    transferred_tags: [],
    transferred_external_refs: [], redundant_external_refs: [],
    transferred_relations_count: 0, redundant_relations_count: 0,
    notes: [],
    ...overrides,
  };
}
