import { render, screen, fireEvent, act, within } from '@testing-library/react';
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { App } from '../../app/App';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary, LibrarySearchQuery, Page } from './types';

const mockAssets: AssetSummary[] = [
  {
    id: '018f0000-0000-7000-8000-000000000001',
    kind: 'software.tool',
    name: 'ripgrep',
    lifecycle: 'active',
    subtitle: 'Fast line-oriented search tool',
    tags: ['cli', 'search', 'rust'],
    updated_at: '2024-03-20T10:00:00Z',
  },
  {
    id: '018f0000-0000-7000-8000-000000000002',
    kind: 'software.app',
    name: '微信开发者工具',
    lifecycle: 'active',
    subtitle: '微信小程序官方集成开发环境',
    tags: ['dev', 'tencent'],
    updated_at: '2024-03-19T10:00:00Z',
  },
  {
    id: '018f0000-0000-7000-8000-000000000003',
    kind: 'media.anime',
    name: "Frieren: Beyond Journey's End",
    lifecycle: 'archived',
    subtitle: 'Elven mage journey',
    tags: ['anime', 'fantasy'],
    updated_at: '2024-03-18T10:00:00Z',
  },
  {
    id: '018f0000-0000-7000-8000-000000000004',
    kind: 'service.saas',
    name: 'OpenAI Platform',
    lifecycle: 'active',
    subtitle: 'API gateway and models',
    tags: ['ai', 'cloud'],
    updated_at: '2024-03-17T10:00:00Z',
  },
];

describe('Search, filters and navigation (P5-04)', () => {
  beforeEach(() => {
    window.location.hash = '';
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('debounces search input by ~300ms before calling transport.searchAssets', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    const searchSpy = vi.spyOn(fake, 'searchAssets');
    setTransport(fake);

    render(<App />);

    // Fast-forward initial load
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    expect(screen.getAllByText('ripgrep')[0]).toBeInTheDocument();

    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });

    // User types keystrokes rapidly
    fireEvent.change(searchInput, { target: { value: 'r' } });
    fireEvent.change(searchInput, { target: { value: 'rip' } });
    fireEvent.change(searchInput, { target: { value: 'ripgrep' } });

    // Immediately after typing, transport.searchAssets has NOT been called yet
    expect(searchSpy).not.toHaveBeenCalled();

    // Advance 150ms - still debouncing
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    expect(searchSpy).not.toHaveBeenCalled();

    // Advance remaining 150ms to cross 300ms threshold
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });

    // Exactly one call triggered with the final debounced query
    expect(searchSpy).toHaveBeenCalledTimes(1);
    expect(searchSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        text: 'ripgrep',
      })
    );
  });

  it('protects against stale search responses overwriting newer responses', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    let resolveQuery1!: (value: Page<AssetSummary>) => void;
    let resolveQuery2!: (value: Page<AssetSummary>) => void;

    const query1Promise = new Promise<Page<AssetSummary>>((res) => {
      resolveQuery1 = res;
    });
    const query2Promise = new Promise<Page<AssetSummary>>((res) => {
      resolveQuery2 = res;
    });

    vi.spyOn(fake, 'searchAssets').mockImplementation((q: LibrarySearchQuery) => {
      if (q.text === 'query1') return query1Promise;
      if (q.text === 'query2') return query2Promise;
      return Promise.resolve({ items: [], offset: 0, limit: 25, total: 0 });
    });
    setTransport(fake);

    render(<App />);
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });

    // Type query1
    fireEvent.change(searchInput, { target: { value: 'query1' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    // Type query2 before query1 resolves
    fireEvent.change(searchInput, { target: { value: 'query2' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    // Query2 resolves first with OpenAI Platform
    await act(async () => {
      resolveQuery2({
        items: [mockAssets[3]],
        offset: 0,
        limit: 25,
        total: 1,
      });
      await vi.runAllTimersAsync();
    });

    expect(screen.getAllByText('OpenAI Platform')[0]).toBeInTheDocument();

    // Query1 resolves late with ripgrep
    await act(async () => {
      resolveQuery1({
        items: [mockAssets[0]],
        offset: 0,
        limit: 25,
        total: 1,
      });
      await vi.runAllTimersAsync();
    });

    // Query1 response is discarded; OpenAI Platform remains displayed!
    expect(screen.getAllByText('OpenAI Platform')[0]).toBeInTheDocument();
    expect(screen.queryByText('ripgrep')).not.toBeInTheDocument();
  });

  it('supports CJK search query without client-side modification', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    const searchSpy = vi.spyOn(fake, 'searchAssets');
    setTransport(fake);

    render(<App />);
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });

    // Type CJK string
    fireEvent.change(searchInput, { target: { value: '微信' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(searchSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        text: '微信',
      })
    );
    expect(screen.getAllByText('微信开发者工具')[0]).toBeInTheDocument();
    expect(screen.queryByText('ripgrep')).not.toBeInTheDocument();
  });

  it('combines text search with kind, tag, and lifecycle filters', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    const searchSpy = vi.spyOn(fake, 'searchAssets');
    setTransport(fake);

    render(<App />);
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    // Select Active + Archived lifecycle
    const lifecycleSelect = screen.getByRole('combobox', { name: /filter by lifecycle/i });
    fireEvent.change(lifecycleSelect, { target: { value: 'active_or_archived' } });
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    // Type search
    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });
    fireEvent.change(searchInput, { target: { value: 'frieren' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(searchSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        text: 'frieren',
        lifecycle: 'active_or_archived',
      })
    );

    expect(screen.getAllByText("Frieren: Beyond Journey's End")[0]).toBeInTheDocument();
  });

  it('clearing search query reverts back to regular library list', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    const listSpy = vi.spyOn(fake, 'listAssets');
    setTransport(fake);

    render(<App />);
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });

    // Search for ripgrep
    fireEvent.change(searchInput, { target: { value: 'ripgrep' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    expect(screen.getAllByText('ripgrep')[0]).toBeInTheDocument();

    // Click clear search button
    const clearButton = screen.getByRole('button', { name: /clear search/i });
    fireEvent.click(clearButton);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    // List query is invoked again and all active assets appear
    expect(listSpy).toHaveBeenCalled();
    expect(screen.getAllByText('OpenAI Platform')[0]).toBeInTheDocument();
  });

  it('focuses search input when Cmd+K shortcut is triggered', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    setTransport(fake);

    render(<App />);
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });
    expect(document.activeElement).not.toBe(searchInput);

    // Trigger Cmd+K
    fireEvent.keyDown(window, { key: 'k', metaKey: true });
    expect(document.activeElement).toBe(searchInput);
  });

  it('opens unified detail view from search results row', async () => {
    const fake = new FakeDesktopTransport(mockAssets);
    setTransport(fake);

    render(<App />);
    await act(async () => {
      await vi.runAllTimersAsync();
    });

    const searchInput = screen.getByRole('searchbox', { name: /search library assets/i });
    fireEvent.change(searchInput, { target: { value: 'ripgrep' } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });

    const row = screen.getAllByText('ripgrep')[0].closest('[role="row"]')!;
    // Double click to open full detail modal
    fireEvent.doubleClick(row);

    await act(async () => {
      await vi.runAllTimersAsync();
    });

    const dialog = screen.getByRole('dialog', { name: /asset detail view/i });
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByRole('heading', { level: 2, name: 'ripgrep' })).toBeInTheDocument();
  });
});
