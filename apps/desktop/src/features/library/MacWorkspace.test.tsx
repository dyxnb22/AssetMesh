import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from '../../app/App';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { setTransport } from './transport';
import type { AssetSummary } from './types';

function media(id: string, name: string): AssetSummary {
  return { id, name, kind: 'media.anime', lifecycle: 'active', subtitle: null, tags: [], revision: 1, updated_at: '2026-09-30T00:00:00Z', details: { module: 'media', asset_id: id, media_type: 'anime', status: 'in_progress', progress: { current: 3, total: 12, unit: 'episodes' } } };
}

describe('Mac workspace interactions', () => {
  let fake: FakeDesktopTransport;
  beforeEach(() => {
    window.history.replaceState(null, '', '#/todos');
    fake = new FakeDesktopTransport([media('a', 'Alpha'), media('b', 'Beta')]);
    setTransport(fake);
  });

  it('closes and reopens the inspector for the selected row, and toggles navigation', async () => {
    render(<App />);
    const list = await screen.findByRole('list', { name: 'Media to-dos' });
    fireEvent.click(within(list).getByRole('button', { name: /^Beta/ }));
    expect(within(screen.getByRole('complementary')).getByRole('heading', { name: 'Beta' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Close Inspector' }));
    expect(screen.queryByRole('complementary')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Toggle inspector' }));
    expect(within(screen.getByRole('complementary')).getByRole('heading', { name: 'Beta' })).toBeInTheDocument();
    await screen.findByText('No relations');
    fireEvent.click(screen.getByRole('button', { name: 'Toggle sidebar' }));
    expect(screen.getByRole('button', { name: 'Toggle sidebar' })).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(screen.getByRole('button', { name: 'Toggle sidebar' }));
    expect(screen.getByRole('button', { name: 'Toggle sidebar' })).toHaveAttribute('aria-expanded', 'true');
  });

  it('searches and sorts the in-progress media scope before pagination and resets filters', async () => {
    fake.assets.push({ ...media('planned', 'Alpha planned'), details: { module: 'media', asset_id: 'planned', media_type: 'anime', status: 'planned' } });
    const search = vi.spyOn(fake, 'searchAssets');
    render(<App />);
    await screen.findByText('2 media items in progress');
    fireEvent.change(screen.getByRole('combobox', { name: 'Sort assets' }), { target: { value: 'name_desc' } });
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search media to-dos' }), { target: { value: 'Alpha' } });
    await screen.findByText('1 media items in progress');
    expect(search).toHaveBeenCalledWith(expect.objectContaining({ text: 'Alpha', modules: ['media'], media_status: 'in_progress', lifecycle: 'active', sort: 'name_desc', offset: 0 }));
    expect(within(screen.getByRole('list')).queryByText('Alpha planned')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Reset Filters' }));
    await screen.findByText('2 media items in progress');
    expect(screen.getByRole('searchbox')).toHaveValue('');
  });

  it('preserves a failed progress draft and shows canonical values after a successful retry', async () => {
    const original = fake.mediaCommand.bind(fake);
    const save = vi.spyOn(fake, 'mediaCommand').mockRejectedValueOnce({ category: 'unavailable', message: 'Disk is busy' }).mockImplementationOnce(async (command) => original(command.action === 'update_progress' ? { ...command, current: 7 } : command));
    render(<App />);
    fireEvent.click(await screen.findByRole('button', { name: 'Update progress' }));
    fireEvent.change(screen.getByRole('spinbutton', { name: 'Current progress' }), { target: { value: '8' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save Progress' }));
    await screen.findByText('Disk is busy');
    expect(screen.getByRole('spinbutton', { name: 'Current progress' })).toHaveValue(8);
    fireEvent.click(screen.getByRole('button', { name: 'Save Progress' }));
    await screen.findByText('Progress updated');
    expect(save).toHaveBeenLastCalledWith({ action: 'update_progress', asset_id: 'a', expected_revision: 1, current: 8, total: 12, unit: 'episodes' });
    expect(screen.queryByRole('spinbutton', { name: 'Current progress' })).not.toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: 'Progress for Alpha' })).toHaveAttribute('aria-valuenow', '58');
    expect(screen.getByRole('progressbar', { name: 'Viewing progress' })).toHaveAttribute('aria-valuenow', '58');
    fireEvent.click(within(screen.getByRole('complementary')).getByRole('button', { name: 'Mark Completed' }));
    await screen.findByText('Completed: Alpha');
    expect(save).toHaveBeenLastCalledWith({ action: 'transition_status', asset_id: 'a', expected_revision: 2, status: 'completed' });
  });

  it('moves selection with the keyboard and focuses search with Cmd+F', async () => {
    render(<App />);
    const list = await screen.findByRole('list', { name: 'Media to-dos' });
    const alpha = within(list).getByRole('button', { name: /^Alpha/ });
    alpha.focus();
    fireEvent.keyDown(alpha, { key: 'ArrowDown' });
    expect(within(list).getByRole('button', { name: /^Beta/ })).toHaveFocus();
    await waitFor(() => expect(within(screen.getByRole('complementary')).getByRole('heading', { name: 'Beta' })).toBeInTheDocument());
    fireEvent.keyDown(window, { key: 'f', metaKey: true });
    expect(screen.getByRole('searchbox')).toHaveFocus();
  });
});
