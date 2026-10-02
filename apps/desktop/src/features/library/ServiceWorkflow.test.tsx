import { render, screen, fireEvent, waitFor, act } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { App } from '../../app/App';
import { AssetDetailView } from './AssetDetailView';
import { ServicesWorkspace } from './ServicesWorkspace';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary, ServiceRecordDto, ServiceRuntimeLogsDto } from './types';

const initialServiceAsset: AssetSummary = {
  id: 'asset-service-001',
  kind: 'service.saas',
  name: 'GitHub Copilot',
  lifecycle: 'active',
  subtitle: 'Cloud AI pair programmer',
  tags: ['ai', 'developer-tool'],
  updated_at: '2024-03-20T10:00:00Z',
};

const localServiceRecord: ServiceRecordDto = {
  module: 'services',
  asset_id: 'asset-service-local',
  service_type: 'local',
  endpoint_url: 'http://127.0.0.1:7861',
  notes: 'local dev gateway',
  project_dir: '/Users/me/gcli2api',
  start_command: 'bash start-local.sh',
};

const initialLocalService: AssetSummary = {
  id: 'asset-service-local',
  kind: 'service.local',
  name: 'gcli2api',
  lifecycle: 'active',
  subtitle: 'local gateway',
  tags: [],
  updated_at: '2024-03-21T10:00:00Z',
  details: localServiceRecord,
};

describe('Service Workflow UI (P5-06)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([initialServiceAsset]);
    setTransport(fakeTransport);
  });

  it('clears subscription provider, plan, money and renewal date together', async () => {
    const initial = await fakeTransport.getAsset(initialServiceAsset.id);
    if (initial.details.module !== 'services') throw new Error('Expected service fixture');
    const command = vi.spyOn(fakeTransport, 'serviceCommand');
    render(<AssetDetailView assetId={initial.id} />);
    fireEvent.click(await screen.findByTestId('edit-service-button'));
    for (const field of ['provider', 'plan', 'cost', 'renews']) {
      fireEvent.change(screen.getByTestId(`service-${field}-input`), { target: { value: '' } });
    }
    fireEvent.click(screen.getByTestId('save-service-button'));
    await waitFor(() => expect(screen.queryByTestId('service-edit-form')).not.toBeInTheDocument());
    expect(command).toHaveBeenCalledWith(expect.objectContaining({
      provider: null, plan: null, cost: null, currency: null, renews_at: null,
    }));
    const fresh = await fakeTransport.getAsset(initial.id);
    expect(fresh.details).toEqual(expect.objectContaining({
      provider: null, plan: null, cost_minor: null, currency: null, renews_at: null,
    }));
    expect(fresh.name).toBe(initial.name);
  });

  it('Flow 1 (Add Subscription): opens creation modal from the subscriptions page, submits valid service input, updates ledger', async () => {
    const serviceCmdSpy = vi.spyOn(fakeTransport, 'serviceCommand');

    render(<App />);

    // Switch to the subscriptions module via rail
    const subscriptionsNavBtn = await screen.findByTestId('nav-subscriptions');
    fireEvent.click(subscriptionsNavBtn);

    // Wait for ledger to load
    await screen.findAllByText('GitHub Copilot');

    // Click "+ Add Subscription" button in header
    const addBtn = screen.getByTestId('new-asset-button');
    expect(addBtn).toHaveTextContent('+ Add Subscription');
    fireEvent.click(addBtn);

    // Modal appears
    await screen.findByTestId('create-service-form');

    // Fill form
    fireEvent.change(screen.getByTestId('service-create-name-input'), {
      target: { value: 'OpenAI API' },
    });
    fireEvent.change(screen.getByTestId('service-create-type-select'), {
      target: { value: 'api' },
    });
    fireEvent.change(screen.getByTestId('service-create-provider-input'), {
      target: { value: 'OpenAI' },
    });
    fireEvent.change(screen.getByTestId('service-create-plan-input'), {
      target: { value: 'Pay-as-you-go' },
    });
    fireEvent.change(screen.getByTestId('service-create-cost-input'), {
      target: { value: '50.00' },
    });
    fireEvent.change(screen.getByTestId('service-create-currency-input'), {
      target: { value: 'USD' },
    });
    fireEvent.change(screen.getByTestId('service-create-cadence-select'), {
      target: { value: 'usage_based' },
    });
    fireEvent.change(screen.getByTestId('service-create-notes-input'), {
      target: { value: 'API key tied to dev account' },
    });
    fireEvent.change(screen.getByTestId('service-create-tags-input'), {
      target: { value: 'api, ai, llm' },
    });

    // Submit
    fireEvent.submit(screen.getByTestId('create-service-form'));

    await waitFor(() => {
      expect(serviceCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'create',
          name: 'OpenAI API',
          service_type: 'api',
          provider: 'OpenAI',
          plan: 'Pay-as-you-go',
          cost: '50.00',
          currency: 'USD',
          billing_cadence: 'usage_based',
          notes: 'API key tied to dev account',
          tags: ['api', 'ai', 'llm'],
        })
      );
    });

    // Modal closes and new asset is listed
    await waitFor(() => {
      expect(screen.queryByTestId('create-service-form')).not.toBeInTheDocument();
      expect(screen.getAllByText('OpenAI API').length).toBeGreaterThan(0);
    });
  });

  it('Flow 2 (Edit Service Subscription): edits plan and cost with canonical read-back and revision increment', async () => {
    const onUpdated = vi.fn();
    const serviceCmdSpy = vi.spyOn(fakeTransport, 'serviceCommand');

    render(
      <AssetDetailView
        assetId="asset-service-001"
        onAssetUpdated={onUpdated}
      />
    );

    await screen.findByTestId('asset-detail-view');

    // Click edit service button
    const editBtn = screen.getByTestId('edit-service-button');
    fireEvent.click(editBtn);

    const form = screen.getByTestId('service-edit-form');
    expect(form).toBeInTheDocument();

    const planInput = screen.getByTestId('service-plan-input');
    const costInput = screen.getByTestId('service-cost-input');

    fireEvent.change(planInput, { target: { value: 'Enterprise' } });
    fireEvent.change(costInput, { target: { value: '21.00' } });

    // Submit
    fireEvent.click(screen.getByTestId('save-service-button'));

    await waitFor(() => {
      expect(serviceCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'update',
          asset_id: 'asset-service-001',
          expected_revision: 1,
          plan: 'Enterprise',
          cost: '21.00',
        })
      );
    });

    // Read back closed form and updated presentation
    await waitFor(() => {
      expect(screen.queryByTestId('service-edit-form')).not.toBeInTheDocument();
      expect(screen.getByText('Enterprise')).toBeInTheDocument();
      expect(screen.getByText(/21.00 USD/)).toBeInTheDocument();
    });

    expect(onUpdated).toHaveBeenCalled();
  });

  it('Flow 3 (Record Renewal): records explicit renewal date and cost, updates detail', async () => {
    const onUpdated = vi.fn();
    const serviceCmdSpy = vi.spyOn(fakeTransport, 'serviceCommand');

    render(
      <AssetDetailView
        assetId="asset-service-001"
        onAssetUpdated={onUpdated}
      />
    );

    await screen.findByTestId('asset-detail-view');

    // Click Record Renewal button
    const renewalBtn = screen.getByTestId('record-renewal-button');
    fireEvent.click(renewalBtn);

    const form = screen.getByTestId('record-renewal-form');
    expect(form).toBeInTheDocument();

    const dateInput = screen.getByTestId('renewal-date-input');
    const costInput = screen.getByTestId('renewal-cost-input');

    fireEvent.change(dateInput, { target: { value: '2024-04-15' } });
    fireEvent.change(costInput, { target: { value: '19.00' } });

    // Submit renewal
    fireEvent.click(screen.getByTestId('submit-renewal-button'));

    await waitFor(() => {
      expect(serviceCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'record_renewal',
          asset_id: 'asset-service-001',
          renews_at: '2024-04-15',
          cost: '19.00',
        })
      );
    });

    // Read back closed form and updated presentation
    await waitFor(() => {
      expect(screen.queryByTestId('record-renewal-form')).not.toBeInTheDocument();
      expect(screen.getByText('2024-04-15')).toBeInTheDocument();
    });

    expect(onUpdated).toHaveBeenCalled();
  });

  it('Flow 4 (Archive): prompts confirmation, executes service archive, makes read-only', async () => {
    const onUpdated = vi.fn();
    const serviceCmdSpy = vi.spyOn(fakeTransport, 'serviceCommand');

    render(
      <AssetDetailView
        assetId="asset-service-001"
        onAssetUpdated={onUpdated}
      />
    );

    await screen.findByTestId('asset-detail-view');

    // Click Archive button
    const archiveBtn = screen.getByTestId('archive-asset-button');
    fireEvent.click(archiveBtn);

    // Confirmation banner is shown
    const confirmBanner = await screen.findByTestId('archive-confirm-banner');
    expect(confirmBanner).toBeInTheDocument();

    // Confirm Archive
    const confirmBtn = screen.getByTestId('confirm-archive-button');
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(serviceCmdSpy).toHaveBeenCalledWith({
        action: 'archive',
        asset_id: 'asset-service-001',
        expected_revision: 1,
      });
    });

    // Read back shows archived banner and active action buttons disappear
    await waitFor(() => {
      expect(screen.getByText(/Archived Asset:/)).toBeInTheDocument();
      expect(screen.queryByTestId('archive-asset-button')).not.toBeInTheDocument();
      expect(screen.queryByTestId('edit-service-button')).not.toBeInTheDocument();
      expect(screen.queryByTestId('record-renewal-button')).not.toBeInTheDocument();
    });

    expect(onUpdated).toHaveBeenCalled();
  });
});

describe('Services page and local service runtime UI', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([initialLocalService, initialServiceAsset]);
    setTransport(fakeTransport);
  });

  it('splits the pages: 服务 lists the local project, 订阅 keeps billed services without process controls', async () => {
    render(<App />);

    fireEvent.click(await screen.findByTestId('nav-services'));
    await screen.findByTestId('services-workspace');
    expect((await screen.findAllByText('gcli2api')).length).toBeGreaterThan(0);
    expect(screen.queryByText('GitHub Copilot')).not.toBeInTheDocument();

    fireEvent.click(await screen.findByTestId('nav-subscriptions'));
    await screen.findAllByText('GitHub Copilot');
    expect(screen.queryByText('gcli2api')).not.toBeInTheDocument();
    expect(screen.queryByTestId('service-start-button')).not.toBeInTheDocument();
  });

  it('saves launch configuration through the local service creation modal without starting anything', async () => {
    const serviceCmdSpy = vi.spyOn(fakeTransport, 'serviceCommand');
    const startSpy = vi.spyOn(fakeTransport, 'serviceRuntimeStart');

    render(<App />);

    fireEvent.click(await screen.findByTestId('nav-services'));
    await screen.findByTestId('services-workspace');

    // Empty of other services: the add button is available in the header too.
    fireEvent.click(screen.getByTestId('new-asset-button'));
    await screen.findByTestId('create-local-service-form');

    fireEvent.change(screen.getByTestId('local-service-create-name-input'), {
      target: { value: 'gcli2api' },
    });
    fireEvent.change(screen.getByTestId('local-service-create-dir-input'), {
      target: { value: '/Users/diaoyuxuan/gcli2api' },
    });
    fireEvent.change(screen.getByTestId('local-service-create-command-input'), {
      target: { value: 'bash start-local.sh' },
    });
    fireEvent.change(screen.getByTestId('local-service-create-url-input'), {
      target: { value: 'http://127.0.0.1:7861' },
    });
    fireEvent.change(screen.getByTestId('local-service-create-notes-input'), {
      target: { value: 'must use start-local.sh, start.sh resets upstream' },
    });

    fireEvent.submit(screen.getByTestId('create-local-service-form'));

    await waitFor(() => {
      expect(serviceCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'create',
          service_type: 'local',
          name: 'gcli2api',
          project_dir: '/Users/diaoyuxuan/gcli2api',
          start_command: 'bash start-local.sh',
          endpoint_url: 'http://127.0.0.1:7861',
          notes: 'must use start-local.sh, start.sh resets upstream',
        })
      );
    });

    // Nothing starts on creation: no runtime start was sent.
    await waitFor(() => {
      expect(screen.queryByTestId('create-local-service-form')).not.toBeInTheDocument();
    });
    expect(startSpy).not.toHaveBeenCalled();
  });

  it('starts and stops a local service from the page, with state badges and logs following the backend', async () => {
    const startSpy = vi.spyOn(fakeTransport, 'serviceRuntimeStart');
    const stopSpy = vi.spyOn(fakeTransport, 'serviceRuntimeStop');
    const openSpy = vi.spyOn(fakeTransport, 'serviceOpenPage');
    fakeTransport.pushRuntimeLog('asset-service-local', 'listening on 127.0.0.1:7861');

    render(<App />);

    fireEvent.click(await screen.findByTestId('nav-services'));
    await screen.findByTestId('services-workspace');

    const row = await screen.findByTestId('service-row');
    expect(row).toHaveTextContent('gcli2api');
    // Row and detail panel both show the state badge.
    expect(screen.getAllByTestId('service-state-stopped').length).toBe(2);

    // Start: the runtime double reports running, so the UI flips to a stop button.
    fireEvent.click(screen.getAllByTestId('service-start-button')[0]);

    await waitFor(() => {
      expect(startSpy).toHaveBeenCalledWith('asset-service-local');
      expect(screen.getAllByTestId('service-state-running').length).toBe(2);
    });
    expect(screen.getAllByTestId('service-stop-button').length).toBe(2);
    expect(screen.queryByTestId('service-start-button')).not.toBeInTheDocument();

    // Open page appears for a running service with an access address.
    expect(screen.getAllByTestId('service-open-page-button').length).toBeGreaterThan(0);

    fireEvent.click(screen.getAllByTestId('service-open-page-button')[0]);
    await waitFor(() => expect(openSpy).toHaveBeenCalledWith('asset-service-local'));

    // Captured output reaches the detail panel.
    await waitFor(() => {
      expect(screen.getByTestId('service-logs-panel')).toHaveTextContent(
        'listening on 127.0.0.1:7861'
      );
    });

    // Stop: back to stopped, start is offered again.
    fireEvent.click(screen.getAllByTestId('service-stop-button')[0]);
    await waitFor(() => {
      expect(stopSpy).toHaveBeenCalledWith('asset-service-local');
      expect(screen.getAllByTestId('service-state-stopped').length).toBe(2);
    });
    expect(screen.getAllByTestId('service-start-button').length).toBe(2);
  });

  it('shows a responding service as running and directs stop-command configuration', async () => {
    // The backend reports `external` when it holds no process but the access
    // address answers on loopback (the real probe is proven by Rust contracts).
    fakeTransport.runtimeStates.set('asset-service-local', 'external');

    render(<App />);

    fireEvent.click(await screen.findByTestId('nav-services'));
    await screen.findByTestId('services-workspace');

    // Row and detail panel both show the external badge.
    await waitFor(() => {
      expect(screen.getAllByTestId('service-state-external').length).toBe(2);
    });
    // The observation offers no process controls: the backend would refuse a
    // start (port conflict) and never stops a process it did not start.
    expect(screen.queryByTestId('service-start-button')).not.toBeInTheDocument();
    for (const button of screen.getAllByTestId('service-stop-button')) expect(button).toBeDisabled();
    expect(screen.queryByTestId('service-external-note')).not.toBeInTheDocument();
    // The page stays reachable, and the note stays honest about what the
    // probe knows: something answers there, AssetMesh did not start it.
    expect(screen.getAllByTestId('service-open-page-button').length).toBeGreaterThan(0);
    expect(screen.getByText('Configure a stop command to enable Stop and Restart')).toBeInTheDocument();
  });

  it('restarts a running service through the restart operation and preserves its configuration', async () => {
    fakeTransport.runtimeStates.set(initialLocalService.id, 'running');
    const restartSpy = vi.spyOn(fakeTransport, 'serviceRuntimeRestart');
    render(<App />);
    fireEvent.click(await screen.findByTestId('nav-services'));
    fireEvent.click(await screen.findByTestId('service-restart-button'));
    await waitFor(() => expect(restartSpy).toHaveBeenCalledWith(initialLocalService.id));
    await waitFor(() => expect(screen.getByTestId('service-restart-button')).toBeEnabled());
    expect((await fakeTransport.getAsset(initialLocalService.id)).details).toMatchObject(localServiceRecord);
  });

  it('binds a stop command from the workspace configuration without starting the existing service', async () => {
    fakeTransport.runtimeStates.set(initialLocalService.id, 'external');
    const commandSpy = vi.spyOn(fakeTransport, 'serviceCommand');
    const startSpy = vi.spyOn(fakeTransport, 'serviceRuntimeStart');
    render(<App />);
    fireEvent.click(await screen.findByTestId('nav-services'));
    fireEvent.click(await screen.findByTestId('service-edit-button'));
    const input = await screen.findByTestId('local-service-stop-command-input');
    await waitFor(() => expect(input).toBeEnabled());
    fireEvent.change(input, { target: { value: 'bash stop-local.sh' } });
    fireEvent.submit(screen.getByTestId('create-local-service-form'));
    await waitFor(() => expect(commandSpy).toHaveBeenCalledWith(expect.objectContaining({
      action: 'update', asset_id: initialLocalService.id, expected_revision: 1,
      start_command: undefined, project_dir: undefined, stop_command: 'bash stop-local.sh',
    })));
    await waitFor(() => expect(screen.queryByTestId('create-local-service-form')).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getAllByTestId('service-stop-button')[0]).toBeEnabled());
    expect(startSpy).not.toHaveBeenCalled();
  });

  it('filters services by runtime state and keeps actions available for the selected service', async () => {
    fakeTransport.runtimeStates.set(initialLocalService.id, 'running');
    render(<App />);
    fireEvent.click(await screen.findByTestId('nav-services'));
    await screen.findByTestId('service-restart-button');
    fireEvent.click(screen.getByRole('button', { name: 'stopped 0' }));
    expect(screen.queryByTestId('service-row')).not.toBeInTheDocument();
    expect(screen.getByText('No matching services')).toBeInTheDocument();
    expect(screen.getByTestId('service-restart-button')).toBeEnabled();
    fireEvent.click(screen.getByRole('button', { name: 'running 1' }));
    expect(screen.getByTestId('service-row')).toHaveTextContent('gcli2api');
  });

  it('shows a start failure reason near the service and recovers the button', async () => {
    fakeTransport.runtimeStartFailures.set('asset-service-local', {
      message: 'local port 7861 is already in use — the service appears to be running outside AssetMesh.',
      category: 'conflict',
    });

    render(<App />);

    fireEvent.click(await screen.findByTestId('nav-services'));
    await screen.findByTestId('services-workspace');

    fireEvent.click((await screen.findAllByTestId('service-start-button'))[0]);

    await waitFor(() => {
      expect(screen.getByTestId('service-action-error')).toHaveTextContent(
        'local port 7861 is already in use'
      );
      expect(screen.getAllByTestId('service-state-failed').length).toBe(2);
    });

    // The failed run keeps the start action available for a retry.
    for (const button of screen.getAllByTestId('service-start-button')) {
      expect(button).toBeEnabled();
    }
  });
  it('edits local metadata and access address while running without sending frozen launch fields', async () => {
    await fakeTransport.serviceRuntimeStart(initialLocalService.id);
    const commandSpy = vi.spyOn(fakeTransport, 'serviceCommand');
    render(<AssetDetailView assetId={initialLocalService.id} />);
    fireEvent.click(await screen.findByTestId('edit-service-button'));
    await waitFor(() => expect(screen.getByTestId('service-start-command-input')).toBeDisabled());
    expect(screen.queryByTestId('service-cost-input')).not.toBeInTheDocument();
    fireEvent.change(screen.getByTestId('service-notes-input'), { target: { value: 'updated note' } });
    fireEvent.change(screen.getByTestId('service-endpoint-input'), { target: { value: 'http://127.0.0.1:9911' } });
    fireEvent.click(screen.getByTestId('save-service-button'));
    await waitFor(() => expect(commandSpy).toHaveBeenCalledWith(expect.objectContaining({
      action: 'update', notes: 'updated note', endpoint_url: 'http://127.0.0.1:9911',
      project_dir: undefined, start_command: undefined, billing_cadence: undefined, auto_renew: undefined,
    })));
    await waitFor(() => expect(screen.queryByTestId('service-endpoint-input')).not.toBeInTheDocument());
    expect((await fakeTransport.getAsset(initialLocalService.id)).details).toMatchObject({
      endpoint_url: 'http://127.0.0.1:9911', notes: 'updated note', start_command: 'bash start-local.sh',
    });
  });

  it('replaces the previous log window when the backend reports a new run with reset sequences', async () => {
    vi.useFakeTimers();
    try {
      let run = 'first-run';
      fakeTransport.serviceRuntimeLogs = vi.fn(async (_id: string, since = 0, runId?: string): Promise<ServiceRuntimeLogsDto> => ({
        run_id: run, dropped: false,
        lines: runId !== run || since < 1 ? [{ seq: 1, timestamp: 'now', stream: 'stdout', text: run }] : [],
      }));
      const noop = () => {};
      render(<ServicesWorkspace data={{ items: [initialLocalService], total: 1, limit: 25, offset: 0 }}
        error={null} loading={false} searchQuery="" page={1} pageSize={25}
        selectedAssetId={initialLocalService.id} runtimeSupported={true} onNewAsset={noop}
        onSelectAsset={noop} onSearchChange={noop} onSelectPage={noop}
        onResetFilters={noop} onRetry={noop} onOpenDetail={noop} />);
      await act(async () => {});
      expect(screen.getByTestId('service-logs-panel')).toHaveTextContent('first-run');
      run = 'second-run';
      fireEvent.click(screen.getAllByTestId('service-start-button')[0]);
      await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
      expect(screen.getByTestId('service-logs-panel')).toHaveTextContent('second-run');
      expect(screen.getByTestId('service-logs-panel')).not.toHaveTextContent('first-run');
      await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
      expect(screen.getByTestId('service-logs-panel').textContent?.match(/second-run/g)).toHaveLength(1);
    } finally {
      vi.useRealTimers();
    }
  });

});
