import { useCallback, useEffect, useRef, useState } from 'react';
import { normalizeDesktopError, type DesktopTransport } from './transport';
import type { AssetDetailDto, AssetSummary, DesktopError, LibraryQuery, MediaStatusCountDto, MutationReceiptDto, Page } from './types';

const CACHE_PAGES = 8;
const COUNTS_MAX_AGE_MS = 60_000;
function remember<T>(cache: Map<string, T>, key: string, value: T) {
  cache.delete(key);
  cache.set(key, value);
  if (cache.size > CACHE_PAGES) cache.delete(cache.keys().next().value!);
}

interface PageState {
  key: string;
  data: Page<AssetSummary> | null;
  loading: boolean;
  error: DesktopError | null;
}
interface CountEntry { data: MediaStatusCountDto[]; at: number }

/** A bounded display cache. Every visit still reads canonical data in the background. */
export function useLibraryPage(transport: DesktopTransport, enabled: boolean, query: LibraryQuery, text: string,
  countsQuery: LibraryQuery | null, view: string) {
  const key = JSON.stringify([view, query, text]);
  const countsKey = countsQuery ? JSON.stringify(countsQuery) : null;
  const pages = useRef(new Map<string, Page<AssetSummary>>());
  const counts = useRef(new Map<string, CountEntry>());
  const pageSeq = useRef(0);
  const countsSeq = useRef(0);
  const mounted = useRef(true);
  const [state, setState] = useState<PageState>({ key: '', data: null, loading: false, error: null });
  const [countState, setCountState] = useState<{ key: string; data: MediaStatusCountDto[] | null } | null>(null);
  const lastMedia = useRef<AssetSummary | null>(null);
  const current = useRef({ key, countsKey, query, text, countsQuery, enabled });
  current.current = { key, countsKey, query, text, countsQuery, enabled };

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; pageSeq.current++; countsSeq.current++; };
  }, []);

  const fetchPage = useCallback(async () => {
    const request = current.current;
    if (!request.enabled) return;
    const seq = ++pageSeq.current;
    const cached = pages.current.get(request.key) ?? null;
    setState({ key: request.key, data: cached, loading: true, error: null });
    try {
      const data = request.text
        ? await transport.searchAssets({ ...request.query, text: request.text })
        : await transport.listAssets(request.query);
      if (!mounted.current || seq !== pageSeq.current || current.current.key !== request.key) return;
      remember(pages.current, request.key, data);
      setState({ key: request.key, data, loading: false, error: null });
    } catch (error) {
      if (!mounted.current || seq !== pageSeq.current || current.current.key !== request.key) return;
      setState({ key: request.key, data: cached, loading: false, error: normalizeDesktopError(error) });
    }
  }, [transport]);

  const fetchCounts = useCallback(async (force = false) => {
    const request = current.current;
    if (!request.enabled || !request.countsKey || !request.countsQuery) return;
    const cached = counts.current.get(request.countsKey);
    if (!force && cached && Date.now() - cached.at < COUNTS_MAX_AGE_MS) return;
    const seq = ++countsSeq.current;
    try {
      const data = await transport.mediaStatusCounts(request.countsQuery);
      if (!mounted.current || seq !== countsSeq.current || current.current.countsKey !== request.countsKey) return;
      remember(counts.current, request.countsKey, { data, at: Date.now() });
      setCountState({ key: request.countsKey, data });
    } catch {
      if (mounted.current && seq === countsSeq.current && current.current.countsKey === request.countsKey) {
        setCountState({ key: request.countsKey, data: cached?.data ?? null });
      }
    }
  }, [transport]);

  useEffect(() => {
    void fetchPage();
    return () => { pageSeq.current++; };
  }, [key, enabled, fetchPage]);
  useEffect(() => {
    void fetchCounts();
    return () => { countsSeq.current++; };
  }, [countsKey, enabled, fetchCounts]);

  // All ordinary mutations and explicit retries invalidate every visited view.
  const refresh = useCallback(async () => {
    pages.current.clear();
    counts.current.clear();
    pageSeq.current++;
    countsSeq.current++;
    setCountState(null);
    void fetchCounts(true);
    await fetchPage();
  }, [fetchPage, fetchCounts]);

  const updateMedia = useCallback(async (receipt: MutationReceiptDto, fresh?: AssetDetailDto) => {
    const request = current.current;
    const page = pages.current.get(request.key);
    const existing = page?.items.find((item) => item.id === fresh?.id);
    const previous = existing ?? (lastMedia.current?.id === fresh?.id ? lastMedia.current : null);
    // Keep projection fields from the summary. Metadata edits need a new canonical summary.
    const canPatch = (receipt.operation.startsWith('media.transition.') || receipt.operation === 'media.update_progress')
      && fresh?.details.module === 'media' && previous?.details?.module === 'media'
      && fresh.kind === previous.kind && fresh.name === previous.name && fresh.lifecycle === previous.lifecycle
      && fresh.details.year === previous.details.year && fresh.revision === receipt.revision;
    if (!canPatch || !fresh || fresh.details.module !== 'media' || !page || !previous || !receipt.asset_ids.includes(fresh.id)) {
      await refresh();
      return;
    }
    const statusChanged = previous.details?.module === 'media' && previous.details.status !== fresh.details.status;
    const updated: AssetSummary = { ...previous, revision: fresh.revision, updated_at: fresh.updated_at,
      details: fresh.details, tags: fresh.tags };
    lastMedia.current = updated;
    const matches = (!request.query.modules?.length || request.query.modules.includes('media'))
      && (!request.query.kinds?.length || request.query.kinds.includes(fresh.kind))
      && (request.query.lifecycle !== 'active' || fresh.lifecycle === 'active')
      && (request.query.lifecycle !== 'archived' || fresh.lifecycle === 'archived')
      && (!request.query.media_status || request.query.media_status === fresh.details.status)
      && (request.query.tags ?? []).every((tag) => fresh.tags.some((value) => value.toLowerCase() === tag.toLowerCase()));
    let items = page.items.filter((item) => item.id !== fresh.id);
    // A full page can also reinsert an item on Undo; a partial page is backfilled below.
    const complete = page.offset === 0 && page.total !== null && page.items.length === page.total;
    if (matches && (existing || complete)) items = [...items, updated];
    const sort = request.query.sort ?? 'updated_desc';
    items.sort((a, b) => {
      const primary = sort === 'updated_desc' ? compareTimestamp(b.updated_at, a.updated_at)
        : sort === 'updated_asc' ? compareTimestamp(a.updated_at, b.updated_at)
        : sort === 'kind_asc' ? compareCodePoints(a.kind, b.kind) || compareName(a.name, b.name)
        : sort === 'name_desc' ? compareName(b.name, a.name) : compareName(a.name, b.name);
      return primary || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
    });
    const patched = { ...page, items, total: page.total === null ? null : page.total + items.length - page.items.length };
    pages.current.clear();
    pageSeq.current++;
    remember(pages.current, request.key, patched);
    setState({ key: request.key, data: patched, loading: false, error: null });
    if (statusChanged) {
      counts.current.clear();
      countsSeq.current++;
      setCountState(null);
      void fetchCounts(true);
    }
    // Unchanged membership preserves name/kind ordering. On the first newest-first
    // page, moving a known row ahead of the old first row also cannot displace
    // it with an unseen row. Other partial pages still need canonical backfill.
    const fixedOrder = sort === 'name_asc' || sort === 'name_desc' || sort === 'kind_asc';
    const movesToFront = sort === 'updated_desc' && page.offset === 0 && page.items.length > 0
      && compareTimestamp(updated.updated_at, page.items[0].updated_at) > 0;
    const safePartialPatch = existing && matches && (fixedOrder || movesToFront);
    if (request.text || (!complete && !safePartialPatch)) void fetchPage();
  }, [fetchPage, fetchCounts, refresh]);

  const data = state.key === key ? state.data : pages.current.get(key) ?? null;
  const statusCounts = countsKey ? (countState?.key === countsKey ? countState.data : counts.current.get(countsKey)?.data ?? null) : null;
  return { data, statusCounts, refresh, updateMedia,
    loading: enabled && (state.key !== key || state.loading), error: state.key === key ? state.error : null };
}

function compareName(a: string, b: string) {
  return compareCodePoints(a.toLowerCase(), b.toLowerCase());
}

function compareTimestamp(a: string, b: string) {
  const milliseconds = Date.parse(a) - Date.parse(b);
  if (milliseconds) return milliseconds;
  // Native RFC3339 timestamps retain nanoseconds; Date.parse stops at milliseconds.
  const fraction = (value: string) => (value.match(/\.(\d+)/)?.[1] ?? '').padEnd(9, '0');
  const left = fraction(a), right = fraction(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

// Match Rust/SQLite's Unicode ordering, including titles outside the BMP.
function compareCodePoints(a: string, b: string) {
  const left = Array.from(a), right = Array.from(b);
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    const difference = left[i].codePointAt(0)! - right[i].codePointAt(0)!;
    if (difference) return difference;
  }
  return left.length - right.length;
}
