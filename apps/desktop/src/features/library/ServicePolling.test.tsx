import { Profiler } from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { setTransport } from './transport';
import { ServicesWorkspace } from './ServicesWorkspace';
import type { AssetSummary } from './types';

const asset: AssetSummary = { id: 'local', name: 'Local project', kind: 'service.local', lifecycle: 'active',
  subtitle: null, tags: [], updated_at: '2026-09-30T00:00:00Z',
  details: { module: 'services', asset_id: 'local', service_type: 'local', start_command: 'npm run dev', project_dir: '/tmp/local-project' } };
const noop = () => {};
function workspace(runtimeSupported = true) {
  return <ServicesWorkspace data={{ items: [asset], total: 1, limit: 25, offset: 0 }}
    error={null} loading={false} searchQuery="" page={1} pageSize={25}
    selectedAssetId={asset.id} runtimeSupported={runtimeSupported} onNewAsset={noop}
    onSelectAsset={noop} onSearchChange={noop} onSelectPage={noop}
    onResetFilters={noop} onRetry={noop} onOpenDetail={noop} />;
}

describe('Service polling work', () => {
  let fake: FakeDesktopTransport;
  beforeEach(() => {
    vi.useFakeTimers();
    fake = new FakeDesktopTransport([asset]);
    setTransport(fake);
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
  });
  afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

  it('does not commit another render for unchanged process snapshots', async () => {
    const commit = vi.fn();
    render(<Profiler id="services" onRender={commit}>{workspace()}</Profiler>);
    await act(async () => {});
    const commits = commit.mock.calls.length;
    await act(async () => { await vi.advanceTimersByTimeAsync(6000); });
    expect(commit).toHaveBeenCalledTimes(commits);
  });

  it('pauses hidden polling, refreshes on return, and updates immediately after starting', async () => {
    const statuses = vi.spyOn(fake, 'serviceRuntimeStatuses');
    const logs = vi.spyOn(fake, 'serviceRuntimeLogs');
    const visible = vi.spyOn(document, 'visibilityState', 'get');
    render(workspace());
    await act(async () => {});
    const beforeHidden = statuses.mock.calls.length;
    const logsBeforeHidden = logs.mock.calls.length;
    visible.mockReturnValue('hidden');
    fireEvent(document, new Event('visibilitychange'));
    await act(async () => { await vi.advanceTimersByTimeAsync(6000); });
    expect(statuses).toHaveBeenCalledTimes(beforeHidden);
    expect(logs).toHaveBeenCalledTimes(logsBeforeHidden);
    visible.mockReturnValue('visible');
    await act(async () => { fireEvent(document, new Event('visibilitychange')); });
    expect(statuses).toHaveBeenCalledTimes(beforeHidden + 1);
    await act(async () => { fireEvent.click(screen.getAllByTestId('service-start-button')[0]); });
    expect(screen.getAllByTestId('service-state-running')).not.toHaveLength(0);
  });

  it('avoids polling an unavailable runtime', async () => {
    const statuses = vi.spyOn(fake, 'serviceRuntimeStatuses');
    const logs = vi.spyOn(fake, 'serviceRuntimeLogs');
    render(workspace(false));
    await act(async () => { await vi.advanceTimersByTimeAsync(6000); });
    expect(statuses).not.toHaveBeenCalled();
    expect(logs).not.toHaveBeenCalled();
  });

  it('preserves an external observation on a failed round and updates after recovery', async () => {
    fake.runtimeStates.set(asset.id, 'external');
    const statuses = vi.spyOn(fake, 'serviceRuntimeStatuses');
    render(workspace());
    await act(async () => {});
    expect(screen.getAllByTestId('service-state-external')).toHaveLength(2);

    statuses.mockRejectedValueOnce({ category: 'unavailable', message: 'Probe budget exhausted' });
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    expect(screen.getAllByTestId('service-state-external')).toHaveLength(2);
    expect(screen.queryByTestId('service-start-button')).not.toBeInTheDocument();

    fake.runtimeStates.set(asset.id, 'stopped');
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    expect(screen.queryByTestId('service-state-external')).not.toBeInTheDocument();
    expect(screen.getAllByTestId('service-state-stopped')).toHaveLength(2);
    expect(screen.getAllByTestId('service-start-button')).toHaveLength(2);
  });

  it('does not overlap slow reads and ignores a late response after unmount', async () => {
    let resolve!: (rows: Awaited<ReturnType<typeof fake.serviceRuntimeStatuses>>) => void;
    const statuses = vi.spyOn(fake, 'serviceRuntimeStatuses').mockReturnValue(new Promise((done) => { resolve = done; }));
    const logs = vi.spyOn(fake, 'serviceRuntimeLogs');
    const view = render(workspace());
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(statuses).toHaveBeenCalledTimes(1);
    expect(logs).toHaveBeenCalledTimes(1);
    view.unmount();
    await act(async () => { resolve([]); await vi.advanceTimersByTimeAsync(10_000); });
    expect(statuses).toHaveBeenCalledTimes(1);
  });
});
