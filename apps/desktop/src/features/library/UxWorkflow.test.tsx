import { act, fireEvent, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from '../../app/App';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { AssetDetailView } from './AssetDetailView';
import { CreateMediaModal } from './CreateMediaModal';
import { setTransport } from './transport';
import type { AssetSummary } from './types';
import { useNavigation } from './useNavigation';

function media(id: string, status = 'in_progress'): AssetSummary {
  return {
    id, kind: 'media.anime', name: `Media ${id}`, lifecycle: 'active', subtitle: null,
    tags: [], revision: 1, updated_at: '2026-09-30T00:00:00Z',
    details: { module: 'media', asset_id: id, media_type: 'anime', status },
  };
}

beforeEach(() => { window.history.replaceState(null, '', '#/'); });

describe('Library usability workflows', () => {
  it('combines search, media status and chosen ordering before pagination', async () => {
    window.history.replaceState(null, '', '#/?module=media&status=in_progress&sort=name_desc&q=Media');
    const fake = new FakeDesktopTransport([
      ...Array.from({ length: 30 }, (_, i) => media(String(i).padStart(2, '0'))),
      ...Array.from({ length: 30 }, (_, i) => media(`planned-${i}`, 'planned')),
    ]);
    setTransport(fake);
    const spy = vi.spyOn(fake, 'searchAssets');
    render(<App />);
    await screen.findByText('Page 1 of 2 (30 total)');
    expect(screen.getAllByRole('row')[0]).toHaveTextContent('Media 29');
    expect(screen.getAllByRole('row')).toHaveLength(25);
    expect(spy).toHaveBeenCalledWith(expect.objectContaining({ media_status: 'in_progress', sort: 'name_desc', text: 'Media' }));
    fireEvent.click(screen.getByRole('button', { name: 'Next Page' }));
    await screen.findByText('Page 2 of 2 (30 total)');
    expect(screen.getAllByRole('row').map((row) => row.textContent)).toEqual([
      expect.stringContaining('Media 04'), expect.stringContaining('Media 03'), expect.stringContaining('Media 02'),
      expect.stringContaining('Media 01'), expect.stringContaining('Media 00'),
    ]);
  });

  it('restores module filters, search, page, selection and scrolling across navigation and reopening', async () => {
    window.history.replaceState(null, '', '#/?module=media&status=in_progress&sort=name_asc&q=Media&page=2&asset=29');
    const fake = new FakeDesktopTransport([
      ...Array.from({ length: 30 }, (_, i) => media(String(i).padStart(2, '0'))),
      { ...media('app'), kind: 'software.app', details: { module: 'unknown' } },
    ]);
    setTransport(fake);
    const view = render(<App />);
    await screen.findByText('Page 2 of 2 (30 total)');
    const ledger = screen.getByLabelText('Asset Ledger');
    ledger.scrollTop = 120;
    fireEvent.scroll(ledger);
    fireEvent.click(screen.getByRole('tab', { name: 'Software' }));
    await screen.findByText('1 asset recorded');
    expect(screen.getByRole('searchbox', { name: 'Search library assets' })).toHaveValue('');
    fireEvent.click(screen.getByRole('tab', { name: 'Media' }));
    await screen.findByText('Page 2 of 2 (30 total)');
    expect(screen.getByRole('combobox', { name: 'Sort assets' })).toHaveValue('name_asc');
    expect(screen.getByRole('searchbox', { name: 'Search library assets' })).toHaveValue('Media');
    expect(screen.getByLabelText('Asset Ledger').scrollTop).toBe(120);
    expect(within(screen.getByRole('complementary')).getByRole('heading', { name: 'Media 29' })).toBeInTheDocument();
    view.unmount();
    window.history.replaceState(null, '', window.location.pathname);
    render(<App />);
    await screen.findByText('Page 2 of 2 (30 total)');
    expect(screen.getByRole('tab', { name: 'Media' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('Asset Ledger').scrollTop).toBe(120);
    expect(within(screen.getByRole('complementary')).getByRole('heading', { name: 'Media 29' })).toBeInTheDocument();
  });

  it.each(['todos', 'relations', 'activity', 'duplicates', 'import-export', 'settings'] as const)('restores the %s section from its URL', (section) => {
    window.history.replaceState(null, '', `#/${section}`);
    const { result } = renderHook(() => useNavigation());
    expect(result.current.nav.section).toBe(section);
  });

  it('shows missing media data and offers creation or filter reset in empty views', async () => {
    window.history.replaceState(null, '', '#/?module=media');
    setTransport(new FakeDesktopTransport([media('empty')]));
    const view = render(<App />);
    const row = await screen.findByRole('row');
    expect(row).toHaveTextContent('No progress recorded');
    expect(row).toHaveTextContent('Unrated');
    view.unmount();
    setTransport(new FakeDesktopTransport([]));
    render(<App />);
    await screen.findByText('Add your first asset to start recording and tracking it.');
    fireEvent.click(screen.getAllByRole('button', { name: /Add Media/ }).at(-1)!);
    expect(await screen.findByRole('dialog', { name: 'Add Media Asset' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Close add media modal' }));
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search library assets' }), { target: { value: 'missing' } });
    await screen.findByText('No matches for “missing”. Try another keyword or reset filters.');
    fireEvent.click(screen.getByRole('button', { name: 'Reset All Filters' }));
    await screen.findByText('Add your first asset to start recording and tracking it.');
    expect(screen.getByRole('tab', { name: 'Media' })).toHaveAttribute('aria-selected', 'true');
  });

  it('offers revision-safe undo from both inspector and detail status controls', async () => {
    window.history.replaceState(null, '', '#/?module=media');
    const fake = new FakeDesktopTransport([media('undo')]);
    setTransport(fake);
    render(<App />);
    const inspector = await screen.findByRole('complementary');
    fireEvent.click(within(inspector).getByRole('button', { name: 'Mark Completed' }));
    await screen.findByText('Completed: Media undo');
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await screen.findByText('Change undone');
    expect((await fake.getAsset('undo')).details).toMatchObject({ status: 'in_progress' });
    fireEvent.click(screen.getByRole('button', { name: 'View Full Details' }));
    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('media-action-complete'));
    const detail = screen.getByTestId('asset-detail-view');
    await within(detail).findByText('Completed: Media undo');
    fireEvent.click(within(detail).getByRole('button', { name: 'Undo' }));
    await within(detail).findByText('Change undone');
    expect((await fake.getAsset('undo')).details).toMatchObject({ status: 'in_progress' });
  });

  it('refreshes the current module when a detail save finishes after closing and navigation', async () => {
    window.history.replaceState(null, '', '#/?module=media');
    const fake = new FakeDesktopTransport([
      media('slow'), { ...media('software'), kind: 'software.app', details: { module: 'unknown' } },
    ]);
    setTransport(fake);
    const original = fake.mediaCommand.bind(fake);
    let finish: (() => void) | undefined;
    const gate = new Promise<void>((resolve) => { finish = resolve; });
    vi.spyOn(fake, 'mediaCommand').mockImplementation(async (command) => { await gate; return original(command); });
    const lists = vi.spyOn(fake, 'listAssets');
    render(<App />);
    await screen.findByRole('row');
    fireEvent.click(screen.getByRole('button', { name: 'View Full Details' }));
    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-progress-button'));
    fireEvent.change(screen.getByTestId('media-progress-current-input'), { target: { value: '7' } });
    fireEvent.click(screen.getByTestId('save-progress-button'));
    fireEvent.keyDown(window, { key: 'Escape' });
    fireEvent.click(screen.getByRole('tab', { name: 'Software' }));
    await screen.findByText('1 asset recorded');
    lists.mockClear();
    await act(async () => { finish?.(); });
    await waitFor(() => expect(lists).toHaveBeenLastCalledWith(expect.objectContaining({ modules: ['software'] })));
    expect(screen.getByRole('row')).toHaveTextContent('Media software');
  });

  it('keeps progress and rating drafts after failure, then saves the same values on retry', async () => {
    const fake = new FakeDesktopTransport([media('draft')]);
    setTransport(fake);
    const spy = vi.spyOn(fake, 'mediaCommand');
    render(<AssetDetailView assetId="draft" />);
    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-progress-button'));
    fireEvent.change(screen.getByTestId('media-progress-current-input'), { target: { value: '8' } });
    fireEvent.change(screen.getByTestId('media-progress-total-input'), { target: { value: '12' } });
    spy.mockRejectedValueOnce({ category: 'unavailable', message: 'Disk busy' });
    fireEvent.click(screen.getByTestId('save-progress-button'));
    await screen.findByText('Disk busy');
    expect(screen.getByTestId('media-progress-current-input')).toHaveValue(8);
    expect(screen.getByTestId('media-progress-total-input')).toHaveValue(12);
    fireEvent.click(screen.getByTestId('save-progress-button'));
    await waitFor(() => expect(screen.queryByTestId('media-progress-current-input')).not.toBeInTheDocument());
    expect(screen.getByTestId('media-progress-text')).toHaveTextContent('8 / 12 episodes');
    fireEvent.click(screen.getByTestId('edit-rating-button'));
    fireEvent.change(screen.getByTestId('media-rating-input'), { target: { value: '9' } });
    spy.mockRejectedValueOnce({ category: 'unavailable', message: 'Disk busy' });
    fireEvent.click(screen.getByTestId('save-rating-button'));
    await screen.findByText('Disk busy');
    expect(screen.getByTestId('media-rating-input')).toHaveValue(9);
    fireEvent.click(screen.getByTestId('save-rating-button'));
    await waitFor(() => expect(screen.queryByTestId('media-rating-input')).not.toBeInTheDocument());
    expect(screen.getByTestId('media-rating-text')).toHaveTextContent('★ 9 / 10');
  });
});

describe('Media creation', () => {
  it('adapts default units and collapses optional movie progress while preserving custom units', async () => {
    const fake = new FakeDesktopTransport([]);
    setTransport(fake);
    const spy = vi.spyOn(fake, 'mediaCommand');
    render(<CreateMediaModal isOpen onClose={() => {}} onCreated={() => {}} />);
    const type = screen.getByTestId('media-create-type-select');
    const unit = screen.getByTestId('media-create-unit-input');
    const progress = unit.closest('details')!;
    expect(unit).toHaveValue('episodes');
    expect(progress).toHaveAttribute('open');
    expect(screen.getByTestId('media-create-rating-input').closest('details')).not.toHaveAttribute('open');
    fireEvent.change(type, { target: { value: 'movie' } });
    expect(unit).toHaveValue('minutes');
    expect(progress).not.toHaveAttribute('open');
    expect(screen.getByRole('button', { name: 'Create Media Asset' })).toBeVisible();
    fireEvent.change(type, { target: { value: 'game' } });
    expect(unit).toHaveValue('hours');
    expect(progress).toHaveAttribute('open');
    fireEvent.change(unit, { target: { value: 'chapters' } });
    fireEvent.change(type, { target: { value: 'tv' } });
    expect(unit).toHaveValue('chapters');
    fireEvent.change(screen.getByTestId('media-create-title-input'), { target: { value: 'New media' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Media Asset' }));
    await waitFor(() => expect(spy).toHaveBeenCalledWith(expect.objectContaining({ action: 'create', progress_unit: undefined, progress_current: undefined, progress_total: undefined })));
  });

  it('retains a new media draft after a failed creation', async () => {
    const fake = new FakeDesktopTransport([]);
    setTransport(fake);
    vi.spyOn(fake, 'mediaCommand').mockRejectedValueOnce({ category: 'unavailable', message: 'Cannot write' });
    const created = vi.fn();
    render(<CreateMediaModal isOpen onClose={() => {}} onCreated={created} />);
    fireEvent.change(screen.getByTestId('media-create-title-input'), { target: { value: 'Draft title' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create Media Asset' }));
    await screen.findByText('Cannot write');
    expect(screen.getByTestId('media-create-title-input')).toHaveValue('Draft title');
    fireEvent.click(screen.getByRole('button', { name: 'Create Media Asset' }));
    await waitFor(() => expect(created).toHaveBeenCalled());
  });
});

it('advances keyboard focus and preserves scroll while completing successive to-dos', async () => {
  window.history.replaceState(null, '', '#/todos');
  setTransport(new FakeDesktopTransport([media('a'), media('b'), media('c')]));
  render(<App />);
  const checkbox = await screen.findByRole('button', { name: 'Complete Media a' });
  const content = screen.getByRole('list', { name: 'Media to-dos' }).parentElement!;
  content.scrollTop = 80;
  fireEvent.scroll(content);
  act(() => checkbox.focus());
  fireEvent.click(checkbox);
  await screen.findByText('Completed: Media a');
  expect(screen.getByRole('button', { name: 'Complete Media b' })).toHaveFocus();
  expect(content.scrollTop).toBe(80);
  fireEvent.click(screen.getByRole('button', { name: 'Complete Media b' }));
  await screen.findByText('Completed: Media b');
  expect(screen.getByRole('button', { name: 'Complete Media c' })).toHaveFocus();
  fireEvent.click(screen.getByRole('button', { name: 'Complete Media c' }));
  await screen.findByText('No media in progress');
  expect(screen.getAllByRole('button', { name: 'Open Media Library' })[0]).toHaveFocus();
});
