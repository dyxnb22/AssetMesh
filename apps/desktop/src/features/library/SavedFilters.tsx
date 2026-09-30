import React, { useState } from 'react';
import { t } from '../../i18n';
import type { ActiveModule, LifecycleOption, SortOption } from './types';

export interface SavedFilterState {
  module: ActiveModule;
  lifecycle: LifecycleOption;
  sort: SortOption;
  kind: string | null;
  tag: string | null;
  search: string;
}

interface SavedFilter {
  name: string;
  state: SavedFilterState;
}

const storageKey = 'assetmesh-saved-filters-v1';
const modules = new Set(['all', 'media', 'software', 'services', 'info']);
const lifecycles = new Set(['active', 'active_or_archived', 'all']);
const sorts = new Set(['updated_desc', 'updated_asc', 'name_asc', 'name_desc', 'kind_asc']);

function loadFilters(): SavedFilter[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(storageKey) ?? '[]');
    if (!Array.isArray(value)) return [];
    return value.filter((item): item is SavedFilter =>
      item !== null && typeof item === 'object' && typeof item.name === 'string' &&
      item.name.trim().length > 0 && item.name.length <= 80 &&
      item.state !== null && typeof item.state === 'object' &&
      modules.has(item.state.module) && lifecycles.has(item.state.lifecycle) &&
      sorts.has(item.state.sort) && typeof item.state.search === 'string' &&
      (item.state.kind === null || typeof item.state.kind === 'string') &&
      (item.state.tag === null || typeof item.state.tag === 'string')).slice(0, 50);
  } catch {
    return [];
  }
}

export const SavedFilters: React.FC<{
  current: SavedFilterState;
  onApply: (state: SavedFilterState) => void;
}> = ({ current, onApply }) => {
  const [filters, setFilters] = useState<SavedFilter[]>(loadFilters);
  const [name, setName] = useState('');
  const [selected, setSelected] = useState('');
  const [error, setError] = useState('');

  const persist = (next: SavedFilter[]) => {
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
      setFilters(next);
      setError('');
      return true;
    } catch {
      setError(t('Could not save filters on this device.'));
      return false;
    }
  };

  return <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
    <label style={{ fontSize: 12, color: 'var(--color-muted)' }}>{t('Saved filters')}{' '}
      <select aria-label={t('Saved filters')} value={selected} onChange={(event) => {
        const next = event.target.value;
        setSelected(next);
        const found = filters.find((filter) => filter.name === next);
        if (found) onApply(found.state);
      }}>
        <option value="">{t('Choose a saved filter')}</option>
        {filters.map((filter) => <option key={filter.name} value={filter.name}>{filter.name}</option>)}
      </select>
    </label>
    <input aria-label={t('Filter name')} value={name} onChange={(event) => setName(event.target.value)}
      placeholder={t('Filter name')} maxLength={80} style={{ width: 120 }} />
    <button type="button" onClick={() => {
      const trimmed = name.trim();
      if (!trimmed) { setError(t('Enter a filter name.')); return; }
      if (filters.some((filter) => filter.name.toLowerCase() === trimmed.toLowerCase())) {
        setError(t('A filter with this name already exists.'));
        return;
      }
      if (filters.length >= 50) { setError(t('Save at most 50 filters.')); return; }
      if (persist([...filters, { name: trimmed, state: current }])) {
        setSelected(trimmed);
        setName('');
      }
    }}>{t('Save current filter')}</button>
    {selected && <button type="button" onClick={() => {
      if (persist(filters.filter((filter) => filter.name !== selected))) setSelected('');
    }}>{t('Delete saved filter')}</button>}
    {error && <span role="alert" style={{ color: 'var(--color-danger)', fontSize: 12 }}>{error}</span>}
  </div>;
};
