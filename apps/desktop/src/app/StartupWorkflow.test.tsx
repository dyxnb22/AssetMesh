import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setLang } from '../i18n';
import { setTransport } from '../features/library/transport';
import { FakeDesktopTransport } from '../test/fake-transport';
import { App } from './App';

function pendingPreferences() {
  let resolve!: (preferences: Record<string, string>) => void;
  const promise = new Promise<Record<string, string>>((done) => { resolve = done; });
  return { promise, resolve };
}

const assets = [{
  id: '019315d0-7a00-7000-8000-000000000001', kind: 'media.anime',
  name: 'Startup library item', lifecycle: 'active' as const,
  subtitle: null, tags: [], updated_at: '2026-09-22T10:00:00Z',
}];

describe('Startup readiness', () => {
  beforeEach(() => {
    window.location.hash = '';
    localStorage.removeItem('assetmesh-theme');
    localStorage.removeItem('assetmesh-saved-filters-v1');
    setLang('en');
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    localStorage.removeItem('assetmesh-theme');
    localStorage.removeItem('assetmesh-saved-filters-v1');
    setLang('en');
  });

  it('follows system appearance changes until an explicit theme is chosen', async () => {
    const appearance = Object.assign(new EventTarget(), { matches: false });
    vi.stubGlobal('matchMedia', vi.fn(() => appearance));
    setTransport(new FakeDesktopTransport(assets));
    render(<App />);
    await screen.findByRole('row', { name: /Startup library item/ });
    expect(document.documentElement.dataset.theme).toBe('light');
    act(() => { appearance.matches = true; appearance.dispatchEvent(new Event('change')); });
    expect(document.documentElement.dataset.theme).toBe('dark');
    fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    fireEvent.click(await screen.findByTestId('theme-light'));
    act(() => { appearance.dispatchEvent(new Event('change')); });
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('shows library data while preferences are pending and saves only after recovery finishes', async () => {
    const fake = new FakeDesktopTransport(assets);
    const pending = pendingPreferences();
    fake.backupPreferences = () => pending.promise;
    const save = vi.spyOn(fake, 'backupSavePreferences');
    setTransport(fake);
    render(<App />);

    await screen.findByRole('row', { name: /Startup library item/ });
    vi.useFakeTimers();
    await act(async () => { vi.advanceTimersByTime(500); });
    expect(save).not.toHaveBeenCalled();

    await act(async () => { pending.resolve({ 'assetmesh-theme': 'dark' }); });
    expect(document.documentElement.dataset.theme).toBe('dark');
    await act(async () => { vi.advanceTimersByTime(300); });
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ 'assetmesh-theme': 'dark' }));
    expect(screen.getByRole('row', { name: /Startup library item/ })).toBeInTheDocument();
  });

  it('keeps choices made while late preferences are being restored', async () => {
    const fake = new FakeDesktopTransport(assets);
    const pending = pendingPreferences();
    fake.backupPreferences = () => pending.promise;
    const save = vi.spyOn(fake, 'backupSavePreferences');
    setTransport(fake);
    render(<App />);

    await screen.findByRole('row', { name: /Startup library item/ });
    fireEvent.click(screen.getByRole('tab', { name: 'Settings' }));
    fireEvent.click(await screen.findByTestId('theme-light'));
    localStorage.setItem('assetmesh-saved-filters-v1', '[]');
    window.dispatchEvent(new Event('assetmesh-preferences-changed'));

    await act(async () => { pending.resolve({
      'assetmesh-theme': 'dark', 'assetmesh-saved-filters-v1': '[{"name":"Older filter"}]',
    }); });
    expect(document.documentElement.dataset.theme).toBe('light');
    expect(localStorage.getItem('assetmesh-saved-filters-v1')).toBe('[]');
    await waitFor(() => expect(save).toHaveBeenCalledWith(expect.objectContaining({
      'assetmesh-theme': 'light', 'assetmesh-saved-filters-v1': '[]',
    })));
  });
});
