/** Validate library payloads at the native bridge, before they enter UI state. */
import type {
  AssetDetailDto, AssetDetailsDto, AssetSummary, DuplicateCandidateDto, DuplicateEvidenceDto,
  ExternalRefDto, InfoRecordDto, MediaRecordDto, MergedRedirectDto, Page, ServiceRecordDto,
  SoftwareRecordDto,
} from './types';

type Guard<T> = (value: unknown) => value is T;
type Shape<T> = { [K in keyof T]-?: Guard<T[K]> };

const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);
const string = (value: unknown): value is string => typeof value === 'string';
const number = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
const boolean = (value: unknown): value is boolean => typeof value === 'boolean';
const nullable = <T>(guard: Guard<T>): Guard<T | null> => (value): value is T | null => value === null || guard(value);
const optional = <T>(guard: Guard<T>): Guard<T | undefined> => (value): value is T | undefined => value === undefined || guard(value);
const maybe = <T>(guard: Guard<T>) => optional(nullable(guard));
const array = <T>(guard: Guard<T>): Guard<T[]> => (value): value is T[] => Array.isArray(value) && value.every(guard);
const oneOf = <T extends string>(...choices: T[]): Guard<T> => (value): value is T =>
  typeof value === 'string' && choices.some((choice) => choice === value);
const shape = <T>(fields: Shape<T>): Guard<T> => (value): value is T => object(value) &&
  Object.entries(fields).every(([key, guard]) => (guard as Guard<unknown>)(value[key]));

const progress = shape<NonNullable<MediaRecordDto['progress']>>({
  unit: maybe(string), current: maybe(number), total: maybe(number),
});
const media = shape<MediaRecordDto>({
  module: oneOf('media'), asset_id: string, media_type: string, status: string,
  rating: maybe(number), year: maybe(number), platform: maybe(string), progress: maybe(progress),
  notes: maybe(string), started_at: maybe(string), completed_at: maybe(string),
});
const software = shape<SoftwareRecordDto>({
  module: oneOf('software'), asset_id: string, category: string, install_source: maybe(string),
  version: maybe(string), install_location: maybe(string), executable_path: maybe(string),
  purpose: maybe(string), notes: maybe(string), discovered_at: maybe(string), installed_at: maybe(string),
  architecture: maybe(string),
});
const service = shape<ServiceRecordDto>({
  module: oneOf('services'), asset_id: string, service_type: string,
  provider: maybe(string), account_label: maybe(string), endpoint_url: maybe(string),
  dashboard_url: maybe(string), domain_name: maybe(string), plan: maybe(string), cost_minor: maybe(number),
  currency: maybe(string), billing_cadence: maybe(string), renews_at: maybe(string), expires_at: maybe(string),
  auto_renew: maybe(boolean), notes: maybe(string), project_dir: maybe(string), start_command: maybe(string),
  stop_command: maybe(string),
});
const info = shape<InfoRecordDto>({
  module: oneOf('info'), asset_id: string, info_type: oneOf('text', 'url', 'api_key'),
  value: string, notes: nullable(string),
});
const redirect = shape<MergedRedirectDto>({ module: oneOf('merged_redirect'), surviving_asset_id: string });
const externalRef = shape<ExternalRefDto>({ namespace: string, external_id: string, source_url: nullable(string) });
const lifecycle = oneOf('active', 'archived', 'merged');
const summary = shape<Omit<AssetSummary, 'details'>>({
  id: string, kind: string, name: string, lifecycle, subtitle: nullable(string),
  tags: array(string), revision: optional(number), updated_at: string,
});
const detail = shape<Omit<AssetDetailDto, 'details'>>({
  id: string, kind: string, name: string, summary: nullable(string), lifecycle,
  revision: number, created_at: string, updated_at: string, archived_at: nullable(string),
  merged_into: nullable(string), tags: array(string), external_refs: array(externalRef),
});

function invalidPayload(): never {
  throw { category: 'unavailable', message: 'The desktop returned an invalid library response.' };
}

function decodeDetails(value: unknown): AssetDetailsDto {
  if (!object(value) || !string(value.module)) return invalidPayload();
  switch (value.module) {
    case 'media': return media(value) ? value : invalidPayload();
    case 'software': return software(value) ? value : invalidPayload();
    case 'services': return service(value) ? value : invalidPayload();
    case 'info': return info(value) ? value : invalidPayload();
    case 'merged_redirect': return redirect(value) ? value : invalidPayload();
    default: return { ...value, original_module: value.module, module: 'unknown' };
  }
}

export function decodeAssetDetail(value: unknown): AssetDetailDto {
  if (!object(value)) return invalidPayload();
  const payload = value.details;
  if (!detail(value)) return invalidPayload();
  return { ...value, details: decodeDetails(payload) };
}

export function decodeAssetSummary(value: unknown): AssetSummary {
  if (!object(value)) return invalidPayload();
  const payload = value.details;
  if (!summary(value)) return invalidPayload();
  return { ...value, details: payload === undefined ? undefined : decodeDetails(payload) };
}

export function decodePage<T>(value: unknown, decode: (item: unknown) => T): Page<T> {
  if (!object(value) || !Array.isArray(value.items) || !number(value.offset) ||
    !number(value.limit) || !nullable(number)(value.total)) return invalidPayload();
  return { items: value.items.map(decode), offset: value.offset, limit: value.limit, total: value.total };
}

const evidence: Guard<DuplicateEvidenceDto> = (value): value is DuplicateEvidenceDto => {
  if (!object(value)) return false;
  switch (value.evidence) {
    case 'same_normalized_name': return string(value.normalized_name) && string(value.kind);
    case 'same_provider': return string(value.provider);
    case 'same_domain': return string(value.domain);
    case 'same_install_location': return string(value.location);
    default: return false;
  }
};

export function decodeDuplicateCandidate(value: unknown): DuplicateCandidateDto {
  if (!object(value) || !array(evidence)(value.evidence) || !array(string)(value.evidence_labels)) return invalidPayload();
  return {
    left: decodeAssetSummary(value.left), right: decodeAssetSummary(value.right),
    evidence: value.evidence, evidence_labels: value.evidence_labels
  };
}
