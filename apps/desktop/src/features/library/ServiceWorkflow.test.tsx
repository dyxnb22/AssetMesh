import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { App } from '../../app/App';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary } from './types';

const initialServiceAsset: AssetSummary = {
  id: 'asset-service-001',
  kind: 'service.saas',
  name: 'GitHub Copilot',
  lifecycle: 'active',
  subtitle: 'Cloud AI pair programmer',
  tags: ['ai', 'developer-tool'],
  updated_at: '2024-03-20T10:00:00Z',
};

describe('Service Workflow UI (P5-06)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([initialServiceAsset]);
    setTransport(fakeTransport);
  });

  it('Flow 1 (Add Service): opens creation modal, submits valid service input, updates ledger', async () => {
    const serviceCmdSpy = vi.spyOn(fakeTransport, 'serviceCommand');

    render(<App />);

    // Switch to Services module via rail
    const servicesNavBtn = await screen.findByTestId('nav-services');
    fireEvent.click(servicesNavBtn);

    // Wait for ledger to load
    await screen.findAllByText('GitHub Copilot');

    // Click "+ Add Service" button in header
    const addBtn = screen.getByTestId('new-asset-button');
    expect(addBtn).toHaveTextContent('+ Add Service');
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
      expect(screen.getByText('rev 2')).toBeInTheDocument();
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
      expect(screen.getByText('rev 2')).toBeInTheDocument();
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
