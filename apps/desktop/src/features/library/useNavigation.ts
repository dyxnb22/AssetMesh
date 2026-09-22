import { useCallback, useEffect, useState } from 'react';
import type {
  ActiveModule,
  ActiveSection,
  LifecycleOption,
  NavigationState,
  SortOption,
} from './types';

const DEFAULT_STATE: NavigationState = {
  section: 'library',
  module: 'all',
  lifecycle: 'active',
  sort: 'updated_desc',
  kind: null,
  tag: null,
  page: 1,
  pageSize: 25,
  selectedAssetId: null,
};

function parseHash(hash: string): NavigationState {
  if (!hash || !hash.startsWith('#/')) {
    return DEFAULT_STATE;
  }

  const raw = hash.slice(2);
  const [pathPart, queryPart] = raw.split('?');
  const params = new URLSearchParams(queryPart || '');

  let section: ActiveSection = 'library';
  if (pathPart === 'relations') section = 'relations';
  else if (pathPart === 'activity') section = 'activity';

  const rawModule = params.get('module');
  const module: ActiveModule =
    rawModule === 'media' || rawModule === 'software' || rawModule === 'services'
      ? rawModule
      : 'all';

  const rawLifecycle = params.get('lifecycle');
  const lifecycle: LifecycleOption =
    rawLifecycle === 'active_or_archived' || rawLifecycle === 'all'
      ? rawLifecycle
      : 'active';

  const rawSort = params.get('sort');
  const sort: SortOption =
    rawSort === 'updated_asc' ||
    rawSort === 'name_asc' ||
    rawSort === 'name_desc' ||
    rawSort === 'kind_asc'
      ? rawSort
      : 'updated_desc';

  const kind = params.get('kind') || null;
  const tag = params.get('tag') || null;
  const page = Math.max(1, parseInt(params.get('page') || '1', 10) || 1);
  const selectedAssetId = params.get('asset') || null;

  return {
    section,
    module,
    lifecycle,
    sort,
    kind,
    tag,
    page,
    pageSize: 25,
    selectedAssetId,
  };
}

function serializeHash(state: NavigationState): string {
  const params = new URLSearchParams();
  if (state.module !== 'all') params.set('module', state.module);
  if (state.lifecycle !== 'active') params.set('lifecycle', state.lifecycle);
  if (state.sort !== 'updated_desc') params.set('sort', state.sort);
  if (state.kind) params.set('kind', state.kind);
  if (state.tag) params.set('tag', state.tag);
  if (state.page > 1) params.set('page', state.page.toString());
  if (state.selectedAssetId) params.set('asset', state.selectedAssetId);

  const queryStr = params.toString();
  const path = state.section === 'library' ? '' : state.section;
  return `#/${path}${queryStr ? '?' + queryStr : ''}`;
}

export function useNavigation() {
  const [nav, setNav] = useState<NavigationState>(() => {
    if (typeof window !== 'undefined') {
      return parseHash(window.location.hash);
    }
    return DEFAULT_STATE;
  });

  useEffect(() => {
    const onHashChange = () => {
      setNav(parseHash(window.location.hash));
    };
    window.addEventListener('hashchange', onHashChange);
    return () => window.removeEventListener('hashchange', onHashChange);
  }, []);

  const updateNav = useCallback((updater: (prev: NavigationState) => NavigationState) => {
    setNav((prev) => {
      const next = updater(prev);
      if (
        prev.section === next.section &&
        prev.module === next.module &&
        prev.lifecycle === next.lifecycle &&
        prev.sort === next.sort &&
        prev.kind === next.kind &&
        prev.tag === next.tag &&
        prev.page === next.page &&
        prev.pageSize === next.pageSize &&
        prev.selectedAssetId === next.selectedAssetId
      ) {
        return prev;
      }
      const newHash = serializeHash(next);
      if (typeof window !== 'undefined' && window.location.hash !== newHash) {
        window.history.replaceState(null, '', newHash);
      }
      return next;
    });
  }, []);

  const setModule = useCallback((module: ActiveModule) => {
    updateNav((prev) => ({
      ...prev,
      section: 'library',
      module,
      kind: null,
      page: 1,
    }));
  }, [updateNav]);

  const setSection = useCallback((section: ActiveSection) => {
    updateNav((prev) => ({
      ...prev,
      section,
      page: 1,
    }));
  }, [updateNav]);

  const setLifecycle = useCallback((lifecycle: LifecycleOption) => {
    updateNav((prev) => ({
      ...prev,
      lifecycle,
      page: 1,
    }));
  }, [updateNav]);

  const setSort = useCallback((sort: SortOption) => {
    updateNav((prev) => ({
      ...prev,
      sort,
      page: 1,
    }));
  }, [updateNav]);

  const setKind = useCallback((kind: string | null) => {
    updateNav((prev) => ({
      ...prev,
      kind,
      page: 1,
    }));
  }, [updateNav]);

  const setTag = useCallback((tag: string | null) => {
    updateNav((prev) => ({
      ...prev,
      tag,
      page: 1,
    }));
  }, [updateNav]);

  const setPage = useCallback((page: number) => {
    updateNav((prev) => ({
      ...prev,
      page: Math.max(1, page),
    }));
  }, [updateNav]);

  const setSelectedAssetId = useCallback((selectedAssetId: string | null) => {
    updateNav((prev) => ({
      ...prev,
      selectedAssetId,
    }));
  }, [updateNav]);

  const resetFilters = useCallback(() => {
    updateNav((prev) => ({
      ...prev,
      module: 'all',
      lifecycle: 'active',
      sort: 'updated_desc',
      kind: null,
      tag: null,
      page: 1,
    }));
  }, [updateNav]);

  return {
    nav,
    setModule,
    setSection,
    setLifecycle,
    setSort,
    setKind,
    setTag,
    setPage,
    setSelectedAssetId,
    resetFilters,
  };
}
