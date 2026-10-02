import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import examples from '../../test/desktop-wire-fixtures.json';
import { TauriTransport } from './transport';
import { decodeAssetDetail, decodeAssetSummary, decodeDuplicateCandidate } from './wire';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('Desktop library wire contract', () => {
  it('consumes the same typed payloads as the Rust DTO round-trip test', () => {
    for (const example of examples) {
      expect(decodeAssetDetail(example)).toEqual(example);
      expect(decodeAssetSummary({ ...example, subtitle: example.summary }).details).toEqual(example.details);
    }
  });

  it('validates nullable progress fields and rejects broken known-module payloads', () => {
    const example = examples[0];
    expect(decodeAssetDetail({
      ...example, details: {
        ...example.details,
        progress: { unit: null, current: null, total: null },
      }
    }).details).toMatchObject({ progress: { unit: null, current: null, total: null } });
    for (const invalid of [null, { ...example.details, year: '2026' }, { ...example.details, progress: { current: '12' } }]) {
      expect(() => decodeAssetDetail({ ...example, details: invalid })).toThrow();
    }
  });

  it('preserves a future module as a read-only unknown payload', () => {
    const details = { module: 'future_module', asset_id: examples[0].id, future_field: ['kept'] };
    expect(decodeAssetDetail({ ...examples[0], details }).details).toEqual({
      ...details, module: 'unknown', original_module: 'future_module',
    });
  });

  it('checks duplicate evidence rather than accepting arbitrary JSON', () => {
    const asset = { ...examples[0], subtitle: examples[0].summary };
    const value = {
      left: asset, right: asset,
      evidence: [{ evidence: 'same_normalized_name', normalized_name: 'wire', kind: 'media.movie' }],
      evidence_labels: ['Same name'],
    };
    expect(decodeDuplicateCandidate(value).evidence).toEqual(value.evidence);
    expect(() => decodeDuplicateCandidate({ ...value, evidence: [null] })).toThrow();
    expect(() => decodeDuplicateCandidate({ ...value, evidence: [{ evidence: 'same_provider', provider: 42 }] })).toThrow();
  });

  it('rejects malformed native responses before detail or list state can use them', async () => {
    const transport = new TauriTransport();
    vi.mocked(invoke).mockResolvedValueOnce({ ...examples[0], details: { module: 'software' } });
    await expect(transport.getAsset(examples[0].id)).rejects.toMatchObject({ category: 'unavailable' });
    vi.mocked(invoke).mockResolvedValueOnce({ items: examples, offset: 0, limit: 25, total: 'six' });
    await expect(transport.listAssets()).rejects.toMatchObject({ category: 'unavailable' });
  });
});
