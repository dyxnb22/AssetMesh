import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { setTransport } from './transport';
import { SoftwareDiscoveryModal } from './SoftwareDiscoveryModal';
import { NavigationRail } from './NavigationRail';
import { ImportExportView } from './ImportExportView';
import { useBackupPreferenceSync } from './useBackupPreferenceSync';
import { useViewScroll } from './useViewScroll';

describe('Personal workflows', () => {
  let fake: FakeDesktopTransport;
  beforeEach(() => { fake = new FakeDesktopTransport(); setTransport(fake); });
  afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

  it('keeps maintenance tabs behind Tools and opens them on demand', () => {
    const selected = vi.fn();
    render(<NavigationRail currentSection="library" currentModule="media" capabilities={null} onSelectModule={vi.fn()} onSelectSection={selected} />);
    expect(screen.getByRole('tab', { name: 'Media' })).toBeVisible();
    expect(screen.getByTestId('nav-duplicates')).not.toBeVisible();
    fireEvent.click(screen.getByText('Tools', { selector: 'summary' }));
    fireEvent.click(screen.getByRole('tab', { name: 'Duplicates' }));
    expect(selected).toHaveBeenCalledWith('duplicates');
  });

  it('previews a picked import directory automatically, without applying it', async () => {
    vi.spyOn(fake, 'pickDirectory').mockResolvedValue('/path/to/valid-bundle');
    const preview = vi.spyOn(fake, 'portableImportPreview');
    const apply = vi.spyOn(fake, 'portableImportApply');
    render(<ImportExportView />);
    fireEvent.click(screen.getByTestId('tab-import'));
    fireEvent.click(screen.getByTestId('import-browse-button'));
    await screen.findByTestId('import-summary');
    expect(preview).toHaveBeenCalledWith('/path/to/valid-bundle');
    expect(apply).not.toHaveBeenCalled();
    expect(screen.getByTestId('apply-import-button')).toBeEnabled();
  });

  it('batch adds only unambiguous new applications and marks them as adopted', async () => {
    fake.candidates.push({ ...fake.candidates[0], disposition: 'potential_duplicate', matched_asset_ids: ['existing'], candidate: { ...fake.candidates[0].candidate, display_name: 'Ambiguous' } });
    const command = vi.spyOn(fake, 'softwareCommand');
    const adopted = vi.fn();
    render(<SoftwareDiscoveryModal isOpen onClose={vi.fn()} onAdopted={adopted} />);
    await screen.findByTestId('discovery-candidate-ripgrep');
    expect(screen.queryByRole('checkbox', { name: 'Select Ambiguous' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select new applications' }));
    fireEvent.click(screen.getByTestId('batch-adopt-button'));
    await screen.findByText('Added 2 applications.');
    expect(command).toHaveBeenCalledTimes(2);
    expect(command.mock.calls.every(([payload]) => payload.action === 'adopt_candidate' && payload.target === 'auto' && payload.expected_revision === undefined)).toBe(true);
    expect(adopted).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId('batch-adopt-button')).toBeDisabled();
    expect(screen.queryByRole('checkbox', { name: 'Select ripgrep' })).not.toBeInTheDocument();
  });

  it('reports partial batch failures and retries only the unsuccessful items', async () => {
    const original = fake.softwareCommand.bind(fake);
    const command = vi.spyOn(fake, 'softwareCommand').mockImplementationOnce(() => Promise.reject({ category: 'conflict', message: 'New matching asset detected' })).mockImplementation(original);
    render(<SoftwareDiscoveryModal isOpen onClose={vi.fn()} onAdopted={vi.fn()} />);
    await screen.findByTestId('discovery-candidate-ripgrep');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Select new applications' }));
    fireEvent.click(screen.getByTestId('batch-adopt-button'));
    await waitFor(() => expect(screen.getByTestId('batch-adopt-notice')).toHaveTextContent('Added 1 applications. ripgrep: New matching asset detected'));
    expect(screen.getByRole('checkbox', { name: 'Select ripgrep' })).toBeChecked();
    fireEvent.click(screen.getByTestId('batch-adopt-button'));
    await waitFor(() => expect(screen.getByTestId('batch-adopt-button')).toBeDisabled());
    expect(command).toHaveBeenCalledTimes(3);
  });

  it('saves preferences on change, coalesces rapid changes, and stays idle afterward', async () => {
    vi.useFakeTimers();
    const save = vi.spyOn(fake, 'backupSavePreferences');
    const { unmount } = renderHook(() => useBackupPreferenceSync(fake, true, 'system', 'en'));
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(save).toHaveBeenCalledTimes(1);
    localStorage.setItem('assetmesh-saved-filters-v1', '[{"name":"Test"}]');
    act(() => { for (let i = 0; i < 20; i++) window.dispatchEvent(new Event('assetmesh-preferences-changed')); });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(save).toHaveBeenCalledTimes(2);
    await act(async () => { await vi.advanceTimersByTimeAsync(60000); });
    expect(save).toHaveBeenCalledTimes(2);
    unmount();
    localStorage.removeItem('assetmesh-saved-filters-v1');
  });

  it('bounds failed preference retries and starts a new attempt after a real change', async () => {
    vi.useFakeTimers();
    const save = vi.spyOn(fake, 'backupSavePreferences').mockRejectedValue(new Error('disk unavailable'));
    renderHook(() => useBackupPreferenceSync(fake, true, 'system', 'en'));
    await act(async () => { await vi.advanceTimersByTimeAsync(60000); });
    expect(save).toHaveBeenCalledTimes(4);
    act(() => { window.dispatchEvent(new Event('assetmesh-preferences-changed')); });
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(save).toHaveBeenCalledTimes(5);
  });

  it('flushes a changed preference before the pending debounce when leaving', async () => {
    vi.useFakeTimers();
    const save = vi.spyOn(fake, 'backupSavePreferences');
    const { unmount } = renderHook(() => useBackupPreferenceSync(fake, true, 'system', 'en'));
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    localStorage.setItem('assetmesh-theme', 'dark');
    act(() => { window.dispatchEvent(new Event('assetmesh-preferences-changed')); });
    unmount();
    expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ 'assetmesh-theme': 'dark' }));
    localStorage.removeItem('assetmesh-theme');
  });

  it('coalesces scroll writes and flushes the final position on leaving a view', () => {
    vi.useFakeTimers();
    function View() { const scroll = useViewScroll('personal-media', true); return <div data-testid="scroll" ref={scroll.ref} onScroll={scroll.onScroll} />; }
    const write = vi.spyOn(Storage.prototype, 'setItem');
    const { unmount } = render(<View />);
    const element = screen.getByTestId('scroll');
    for (let i = 1; i <= 20; i++) { element.scrollTop = i * 10; fireEvent.scroll(element); }
    expect(write).not.toHaveBeenCalled();
    act(() => { vi.advanceTimersByTime(300); });
    expect(write).toHaveBeenCalledTimes(1);
    element.scrollTop = 500; fireEvent.scroll(element);
    unmount();
    expect(JSON.parse(localStorage.getItem('assetmesh-view-scroll-v1')!)).toEqual({ 'personal-media': 500 });
  });
});
