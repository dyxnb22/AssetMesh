import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDesktopTransport } from '../test/fake-transport';
import { setTransport } from '../features/library/transport';
import type { AssetSummary, Page } from '../features/library/types';
import { App } from './App';

function media(id: string): AssetSummary {
  return { id, name: `Media ${id}`, kind: 'media.anime', lifecycle: 'active',
    subtitle: null, tags: [], revision: 1, updated_at: '2026-09-30T00:00:00Z',
    details: { module: 'media', asset_id: id, media_type: 'anime', status: 'in_progress',
      progress: { current: 3, total: 12, unit: 'episodes' } } };
}

describe('Library interaction performance', () => {
  let fake: FakeDesktopTransport;
  beforeEach(() => {
    window.history.replaceState(null, '', '#/?module=media');
    fake = new FakeDesktopTransport([media('Alpha'), media('Beta')]);
    setTransport(fake);
  });

  it('finishes loading the list without waiting for media statistics', async () => {
    vi.spyOn(fake, 'mediaStatusCounts').mockReturnValue(new Promise(() => {}));
    render(<App />);
    await screen.findByRole('row', { name: /Media Alpha/ });
    expect(screen.queryByText('Loading assets...')).not.toBeInTheDocument();
    expect(screen.getByText('2 assets recorded')).toBeInTheDocument();
  });

  it('reuses status counts across sorting and paging', async () => {
    fake.assets = Array.from({ length: 26 }, (_, i) => media(String(i)));
    const counts = vi.spyOn(fake, 'mediaStatusCounts');
    render(<App />);
    await screen.findByText('26 assets recorded');
    await waitFor(() => expect(counts).toHaveBeenCalledTimes(1));
    fireEvent.change(screen.getByRole('combobox', { name: 'Sort assets' }), { target: { value: 'name_asc' } });
    await screen.findByText('26 assets recorded');
    fireEvent.click(screen.getByRole('button', { name: 'Next Page' }));
    await screen.findByText('Page 2 of 2 (26 total)');
    await screen.findByText('26 assets recorded');
    expect(counts).toHaveBeenCalledTimes(1);
  });

  it('shows a cached module while its background refresh is pending, then shows fresh data', async () => {
    const original = fake.listAssets.bind(fake);
    const list = vi.spyOn(fake, 'listAssets');
    render(<App />);
    await screen.findByText('2 assets recorded');
    fireEvent.click(screen.getByRole('tab', { name: 'Software' }));
    await screen.findByText('No assets found');
    let resolve!: (page: Page<AssetSummary>) => void;
    const pending = new Promise<Page<AssetSummary>>((done) => { resolve = done; });
    list.mockImplementation((query) => query?.modules?.includes('media') ? pending : original(query));
    fireEvent.click(screen.getByRole('tab', { name: 'Media' }));
    expect(screen.getByRole('row', { name: /Media Alpha/ })).toBeInTheDocument();
    expect(screen.queryByRole('row', { name: /Media Gamma/ })).not.toBeInTheDocument();
    await act(async () => { resolve({ items: [media('Gamma')], total: 1, offset: 0, limit: 25 }); });
    expect(screen.getByRole('row', { name: /Media Gamma/ })).toBeInTheDocument();
    expect(screen.queryByRole('row', { name: /Media Alpha/ })).not.toBeInTheDocument();
  });

  it('updates canonical progress and completes a fully loaded to-do page without querying it again', async () => {
    window.history.replaceState(null, '', '#/todos');
    const list = vi.spyOn(fake, 'listAssets');
    render(<App />);
    await screen.findByText('2 media items in progress');
    const calls = list.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: 'Update progress' }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Current progress' }), { target: { value: '7' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save Progress' }));
    await screen.findByText('Progress updated');
    expect(screen.getByRole('progressbar', { name: 'Progress for Media Alpha' })).toHaveAttribute('aria-valuenow', '58');
    expect(list).toHaveBeenCalledTimes(calls);
    fireEvent.click(screen.getByRole('button', { name: 'Complete Media Alpha' }));
    await screen.findByText('Completed: Media Alpha');
    expect(within(screen.getByRole('list')).queryByText('Media Alpha')).not.toBeInTheDocument();
    expect(screen.getByText('1 media items in progress')).toBeInTheDocument();
    expect(list).toHaveBeenCalledTimes(calls);
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await screen.findByRole('button', { name: 'Complete Media Alpha' });
    expect(list).toHaveBeenCalledTimes(calls);
  });

  it('backfills a partial page after completion while immediately removing the completed item', async () => {
    window.history.replaceState(null, '', '#/todos');
    fake = new FakeDesktopTransport(Array.from({ length: 26 }, (_, i) => media(String(i).padStart(2, '0'))));
    setTransport(fake);
    render(<App />);
    await screen.findByText('26 media items in progress');
    const first = within(screen.getByRole('list')).getAllByRole('button', { name: /^Complete/ })[0];
    const removedName = first.getAttribute('aria-label')!;
    const original = fake.listAssets.bind(fake);
    let resolve!: (page: Page<AssetSummary>) => void;
    vi.spyOn(fake, 'listAssets').mockReturnValue(new Promise((done) => { resolve = done; }));
    fireEvent.click(first);
    await screen.findByText(/^Completed:/);
    expect(screen.queryByRole('button', { name: removedName })).not.toBeInTheDocument();
    expect(within(screen.getByRole('list')).getAllByRole('listitem')).toHaveLength(24);
    await act(async () => { resolve(await original({ modules: ['media'], lifecycle: 'active', media_status: 'in_progress', sort: 'updated_desc', offset: 0, limit: 25 })); });
    expect(within(screen.getByRole('list')).getAllByRole('listitem')).toHaveLength(25);
    expect(screen.getByText('Page 1 of 1 (25 total)')).toBeInTheDocument();
  });

  it('updates progress on the first recently-updated page without rereading unseen items', async () => {
    window.history.replaceState(null, '', '#/todos');
    fake = new FakeDesktopTransport(Array.from({ length: 26 }, (_, i) => media(String(i).padStart(2, '0'))));
    setTransport(fake);
    const list = vi.spyOn(fake, 'listAssets');
    render(<App />);
    await screen.findByText('26 media items in progress');
    const calls = list.mock.calls.length;
    fireEvent.click(within(screen.getByRole('list')).getByRole('button', { name: /^Media 02 / }));
    fireEvent.click(screen.getByRole('button', { name: 'Update progress' }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Current progress' }), { target: { value: '7' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save Progress' }));
    await screen.findByText('Progress updated');
    expect(list).toHaveBeenCalledTimes(calls);
    expect(within(screen.getByRole('list')).getAllByRole('listitem')[0]).toHaveTextContent('Media 02');
    expect(screen.getByText('26 media items in progress')).toBeInTheDocument();
  });

  it('preserves backend timestamp precision when reordering patched rows', async () => {
    window.history.replaceState(null, '', '#/todos');
    fake = new FakeDesktopTransport([
      { ...media('a'), updated_at: '2026-09-30T00:00:00.000000008+00:00' },
      { ...media('z'), updated_at: '2026-09-30T00:00:00.000000007+00:00' },
    ]);
    setTransport(fake);
    const original = fake.mediaCommand.bind(fake);
    vi.spyOn(fake, 'mediaCommand').mockImplementation(async (command) => {
      const receipt = await original(command);
      const detail = fake.details.get('z')!;
      detail.updated_at = '2026-09-30T00:00:00.000000009+00:00';
      return receipt;
    });
    render(<App />);
    await screen.findByText('2 media items in progress');
    fireEvent.click(within(screen.getByRole('list')).getByRole('button', { name: /^Media z / }));
    fireEvent.click(screen.getByRole('button', { name: 'Update progress' }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Current progress' }), { target: { value: '7' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save Progress' }));
    await screen.findByText('Progress updated');
    expect(within(screen.getByRole('list')).getAllByRole('listitem')[0]).toHaveTextContent('Media z');
  });

  it('invalidates other cached modules after a mutation and ignores a read started before that mutation', async () => {
    window.history.replaceState(null, '', '#/todos');
    render(<App />);
    await screen.findByText('2 media items in progress');
    fireEvent.click(screen.getByRole('tab', { name: 'Media' }));
    await screen.findByText('2 assets recorded');
    const stale = structuredClone(await fake.listAssets({ modules: ['media'], lifecycle: 'active', media_status: 'in_progress', sort: 'updated_desc', offset: 0, limit: 25 }));
    const original = fake.listAssets.bind(fake);
    let resolve!: (page: Page<AssetSummary>) => void;
    const list = vi.spyOn(fake, 'listAssets').mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    fireEvent.click(screen.getByRole('tab', { name: 'To-do List' }));
    expect(screen.getByRole('list')).toHaveTextContent('Media Alpha');
    // The detail dialog can save while a cached list's background read is pending.
    fireEvent.click(screen.getByRole('button', { name: 'View Full Details' }));
    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('media-action-complete'));
    await screen.findByText('1 media items in progress');
    fireEvent.click(screen.getByRole('button', { name: 'Close detail view' }));
    await act(async () => { resolve(stale); });
    expect(screen.queryByRole('button', { name: 'Complete Media Alpha' })).not.toBeInTheDocument();
    expect(screen.getByText('1 media items in progress')).toBeInTheDocument();
    let freshResolve!: (page: Page<AssetSummary>) => void;
    list.mockImplementationOnce(() => new Promise((done) => { freshResolve = done; }));
    fireEvent.click(screen.getByRole('tab', { name: 'Media' }));
    expect(screen.queryByRole('row', { name: /Media Alpha/ })).not.toBeInTheDocument();
    await act(async () => { freshResolve(await original({ modules: ['media'], lifecycle: 'active', sort: 'updated_desc', offset: 0, limit: 25 })); });
    fireEvent.click(screen.getByRole('row', { name: /Media Alpha/ }));
    expect(within(screen.getByRole('complementary')).getByRole('combobox', { name: 'Media Status' })).toHaveValue('completed');
  });
});
