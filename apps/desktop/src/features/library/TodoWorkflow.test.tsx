import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from '../../app/App';
import { setTransport } from './transport';
import type { AssetSummary } from './types';
import { FakeDesktopTransport } from '../../test/fake-transport';

function mediaAsset(id: string, status = 'in_progress', lifecycle: AssetSummary['lifecycle'] = 'active'): AssetSummary {
  return {
    id,
    kind: 'media.anime',
    name: `Media ${id}`,
    lifecycle,
    subtitle: null,
    tags: [],
    revision: 1,
    updated_at: '2026-09-30T01:00:00Z',
    details: {
      module: 'media', asset_id: id, media_type: 'anime', status,
      progress: { current: 3, total: 12, unit: 'episodes' },
    },
  };
}

describe('Media to-do workflow', () => {
  let fake: FakeDesktopTransport;

  beforeEach(() => {
    window.location.hash = '#/todos';
    fake = new FakeDesktopTransport([
      mediaAsset('watching'), mediaAsset('planned', 'planned'),
      mediaAsset('finished', 'completed'), mediaAsset('paused', 'paused'),
      mediaAsset('dropped', 'dropped'), mediaAsset('archived', 'in_progress', 'archived'),
      mediaAsset('merged', 'in_progress', 'merged'),
      { ...mediaAsset('software'), kind: 'software.app', details: { module: 'unknown' } },
    ]);
    setTransport(fake);
  });

  function list() {
    return screen.getByRole('list', { name: 'Media to-dos' });
  }

  it('opens from its URL and lists only active media in progress, independently of library filters', async () => {
    window.location.hash = '#/todos?module=software&status=completed&kind=software.app&tag=missing&lifecycle=all&q=missing';
    const searchSpy = vi.spyOn(fake, 'searchAssets');
    render(<App />);
    await screen.findByText('1 media items in progress');
    expect(screen.getByRole('tab', { name: 'To-do List' })).toHaveAttribute('aria-selected', 'true');
    expect(within(list()).getAllByRole('listitem')).toHaveLength(1);
    expect(list()).toHaveTextContent('Media watching');
    expect(screen.getByRole('progressbar', { name: 'Progress for Media watching' })).toHaveAttribute('aria-valuenow', '25');
    expect(searchSpy).not.toHaveBeenCalled();
  });

  it('completes media from the row action, removes the to-do, and undoes using the returned revision', async () => {
    const spy = vi.spyOn(fake, 'mediaCommand');
    render(<App />);
    const checkbox = await screen.findByRole('button', { name: 'Complete Media watching' });
    fireEvent.click(checkbox);
    await screen.findByText('Completed: Media watching');
    expect(screen.queryByRole('button', { name: /^Complete Media/ })).not.toBeInTheDocument();
    expect(screen.getByText('No media in progress')).toBeInTheDocument();
    expect((await fake.getAsset('watching')).details).toMatchObject({ status: 'completed' });
    expect(spy).toHaveBeenCalledWith({ action: 'transition_status', asset_id: 'watching', status: 'completed', expected_revision: 1 });

    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await screen.findByRole('button', { name: 'Complete Media watching' });
    expect(spy).toHaveBeenLastCalledWith({ action: 'transition_status', asset_id: 'watching', status: 'in_progress', expected_revision: 2 });
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: 'Media' }));
    await screen.findByRole('heading', { name: 'Media Library' });
    expect(fake.assets.find((asset) => asset.id === 'watching')?.details).toMatchObject({ status: 'in_progress' });
  });

  it('moves media back to planned, preserves its progress, and can undo', async () => {
    render(<App />);
    fireEvent.click(await screen.findByLabelText('More actions for Media watching'));
    fireEvent.click(screen.getByRole('button', { name: 'Move Media watching to planned' }));
    await screen.findByText('Moved to planned: Media watching');
    expect(screen.queryByRole('button', { name: /^Complete Media/ })).not.toBeInTheDocument();
    expect((await fake.getAsset('watching')).details).toMatchObject({ status: 'planned', progress: { current: 3, total: 12 } });
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await screen.findByRole('button', { name: 'Complete Media watching' });
    expect((await fake.getAsset('watching')).details).toMatchObject({ status: 'in_progress', progress: { current: 3, total: 12 } });
  });

  it('reflects media changes from the library and the detail dialog', async () => {
    window.location.hash = '#/?module=media&status=planned';
    render(<App />);
    const inspector = await screen.findByRole('complementary', { name: 'Asset Inspector' });
    const statusSelect = await within(inspector).findByRole('combobox', { name: 'Media Status' });
    fireEvent.change(statusSelect, { target: { value: 'in_progress' } });
    await waitFor(() => expect(fake.assets.find((asset) => asset.id === 'planned')?.details).toMatchObject({ status: 'in_progress' }));
    fireEvent.click(screen.getByRole('tab', { name: 'To-do List' }));
    await screen.findByText('2 media items in progress');
    fireEvent.click(within(list()).getByRole('button', { name: /^Media planned/ }));
    fireEvent.click(screen.getByRole('button', { name: 'View Full Details' }));
    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('media-action-complete'));
    await screen.findByText('1 media items in progress');
    expect(within(list()).queryByText('Media planned')).not.toBeInTheDocument();
  });

  it('keeps the to-do when a command fails and allows a retry', async () => {
    const spy = vi.spyOn(fake, 'mediaCommand').mockRejectedValueOnce({ category: 'unavailable', message: 'Storage is busy.' });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: 'Complete Media watching' }));
    await screen.findByText('Storage is busy.');
    expect(screen.getByRole('button', { name: /^Complete Media/ })).toBeEnabled();
    expect((await fake.getAsset('watching')).details).toMatchObject({ status: 'in_progress' });
    fireEvent.click(screen.getByRole('button', { name: /^Complete Media/ }));
    await screen.findByText('Completed: Media watching');
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('refreshes stale revisions and prevents undo from overwriting a later media edit', async () => {
    render(<App />);
    const checkbox = await screen.findByRole('button', { name: 'Complete Media watching' });
    await fake.mediaCommand({ action: 'update_progress', asset_id: 'watching', current: 5, expected_revision: 1 });
    fireEvent.click(checkbox);
    await screen.findByText('This media item changed. The list has been refreshed; try again.');
    expect((await fake.getAsset('watching')).details).toMatchObject({ status: 'in_progress' });
    fireEvent.click(screen.getByRole('button', { name: /^Complete Media/ }));
    await screen.findByText('Completed: Media watching');
    const completed = await fake.getAsset('watching');
    await fake.mediaCommand({ action: 'transition_status', asset_id: 'watching', status: 'paused', expected_revision: completed.revision });
    fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await screen.findByText('This media item changed. The list has been refreshed; try again.');
    expect(screen.queryByRole('button', { name: 'Undo' })).not.toBeInTheDocument();
    expect((await fake.getAsset('watching')).details).toMatchObject({ status: 'paused' });
  });

  it('returns to the previous page after completing the last to-do on the final page', async () => {
    fake = new FakeDesktopTransport(Array.from({ length: 26 }, (_, i) => mediaAsset(String(i))));
    setTransport(fake);
    render(<App />);
    await screen.findByText('Page 1 of 2 (26 total)');
    fireEvent.click(screen.getByRole('button', { name: 'Next Page' }));
    await screen.findByText('Page 2 of 2 (26 total)');
    expect(within(list()).getAllByRole('listitem')).toHaveLength(1);
    fireEvent.click(screen.getByRole('button', { name: /^Complete Media/ }));
    await screen.findByText('Page 1 of 1 (25 total)');
    expect(within(list()).getAllByRole('listitem')).toHaveLength(25);
    expect(screen.getByRole('button', { name: 'Next Page' })).toBeDisabled();
  });

  it('blocks repeated commands while saving and refreshes the view navigated to during the save', async () => {
    const original = fake.mediaCommand.bind(fake);
    let resolveSave: (() => void) | undefined;
    const saved = new Promise<void>((resolve) => { resolveSave = resolve; });
    const spy = vi.spyOn(fake, 'mediaCommand').mockImplementation(async (command) => {
      await saved;
      return original(command);
    });
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: 'Complete Media watching' }));
    expect(screen.getByRole('button', { name: /^Complete Media/ })).toBeDisabled();
    fireEvent.click(screen.getByLabelText('More actions for Media watching'));
    fireEvent.click(screen.getByRole('button', { name: 'Move Media watching to planned' }));
    expect(spy).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('tab', { name: 'Media' }));
    await screen.findByRole('heading', { name: 'Media Library' });
    resolveSave?.();
    await waitFor(() => expect(fake.assets.find((asset) => asset.id === 'watching')?.details).toMatchObject({ status: 'completed' }));
    fireEvent.click(screen.getByRole('row', { name: /Media watching/ }));
    await waitFor(() => expect(within(screen.getByRole('complementary')).getByRole('combobox', { name: 'Media Status' })).toHaveValue('completed'));
  });

  it('shows load failures with retry and gives the empty list a media library link', async () => {
    fake = new FakeDesktopTransport([]);
    setTransport(fake);
    vi.spyOn(fake, 'listAssets').mockRejectedValueOnce({ category: 'unavailable', message: 'Cannot load media.' });
    render(<App />);
    await screen.findByText('Cannot load media.');
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await screen.findByText('No media in progress');
    fireEvent.click(screen.getAllByRole('button', { name: 'Open Media Library' })[0]);
    await screen.findByRole('heading', { name: 'Media Library' });
    expect(screen.getByRole('tab', { name: 'Media' })).toHaveAttribute('aria-selected', 'true');
  });
});
