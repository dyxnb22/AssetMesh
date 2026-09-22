import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary, MutationReceiptDto } from './types';

const sampleSoftwareAsset: AssetSummary = {
  id: 'asset-sw-001',
  kind: 'software.tool',
  name: 'ripgrep',
  lifecycle: 'active',
  subtitle: 'Fast line-oriented search tool',
  tags: ['cli', 'search', 'rust'],
  updated_at: '2024-03-20T10:00:00Z',
};

describe('Desktop Mutation Pattern & Receipts (P5-05)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([sampleSoftwareAsset]);
    setTransport(fakeTransport);
  });

  it('Path 1 (Success): edit draft -> submit -> receipt -> read-back -> UI refreshed and edit mode closed', async () => {
    const onAssetUpdated = vi.fn();
    const softwareCmdSpy = vi.spyOn(fakeTransport, 'softwareCommand');
    const getAssetSpy = vi.spyOn(fakeTransport, 'getAsset');

    render(
      <AssetDetailView
        assetId="asset-sw-001"
        onAssetUpdated={onAssetUpdated}
      />
    );

    // 1. Wait for detail to load initially
    await screen.findByTestId('asset-detail-view');
    expect(screen.getByText('Productivity tool')).toBeInTheDocument();
    expect(screen.getByText('rev 1')).toBeInTheDocument();
    expect(getAssetSpy).toHaveBeenCalledTimes(1);

    // 2. Open edit mode
    const editBtn = screen.getByTestId('edit-software-button');
    fireEvent.click(editBtn);

    const form = screen.getByTestId('software-edit-form');
    expect(form).toBeInTheDocument();

    const purposeInput = screen.getByTestId('software-purpose-input') as HTMLInputElement;
    const notesInput = screen.getByTestId('software-notes-input') as HTMLTextAreaElement;

    expect(purposeInput.value).toBe('Productivity tool');

    // 3. Edit draft
    fireEvent.change(purposeInput, { target: { value: 'High performance regex searcher' } });
    fireEvent.change(notesInput, { target: { value: 'Used across all workspaces' } });

    // 4. Submit
    const saveBtn = screen.getByTestId('save-software-button');
    fireEvent.click(saveBtn);

    // 5. Verify transport received mutation with expected revision
    await waitFor(() => {
      expect(softwareCmdSpy).toHaveBeenCalledWith({
        action: 'update_metadata',
        asset_id: 'asset-sw-001',
        expected_revision: 1,
        purpose: 'High performance regex searcher',
        notes: 'Used across all workspaces',
      });
    });

    // 6. Verify read-back occurred (getAsset called again after softwareCommand)
    await waitFor(() => {
      expect(getAssetSpy).toHaveBeenCalledTimes(3);
    });

    // 7. Verify UI state updated from read-back, edit form exited
    await waitFor(() => {
      expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
      expect(screen.getByText('High performance regex searcher')).toBeInTheDocument();
      expect(screen.getByText('Used across all workspaces')).toBeInTheDocument();
      expect(screen.getByText('rev 2')).toBeInTheDocument();
    });

    // 8. Verify receipt notice and onAssetUpdated notification
    expect(screen.getByTestId('mutation-receipt-badge')).toHaveTextContent('Saved successfully (rev 2)');
    expect(onAssetUpdated).toHaveBeenCalledWith(
      expect.objectContaining({
        operation: 'software.update_metadata',
        asset_ids: ['asset-sw-001'],
        revision: 2,
        changed: true,
      })
    );
  });

  it('Path 2 (Validation): input failure -> invalid_input banner -> draft preserved -> no auto-retry', async () => {
    vi.spyOn(fakeTransport, 'softwareCommand').mockRejectedValueOnce({
      category: 'invalid_input',
      message: 'Purpose exceeds maximum character limit of 100',
    });

    render(<AssetDetailView assetId="asset-sw-001" />);

    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-software-button'));

    const purposeInput = screen.getByTestId('software-purpose-input') as HTMLInputElement;
    fireEvent.change(purposeInput, { target: { value: 'A'.repeat(120) } });

    // Submit
    fireEvent.click(screen.getByTestId('save-software-button'));

    // Validation banner appears
    const validationBanner = await screen.findByTestId('mutation-validation-banner');
    expect(validationBanner).toHaveTextContent('Purpose exceeds maximum character limit of 100');
    expect(validationBanner).toHaveTextContent('invalid_input');

    // Draft is PRESERVED
    expect(purposeInput.value).toBe('A'.repeat(120));
    expect(screen.getByTestId('software-edit-form')).toBeInTheDocument();
  });

  it('Path 3 (Stale Revision Conflict): conflict banner displayed -> draft preserved -> no auto-retry -> manual reload', async () => {
    vi.spyOn(fakeTransport, 'softwareCommand').mockRejectedValueOnce({
      category: 'stale_revision',
      message: 'stale revision: expected 1, actual 2',
    });

    render(<AssetDetailView assetId="asset-sw-001" />);

    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-software-button'));

    const purposeInput = screen.getByTestId('software-purpose-input') as HTMLInputElement;
    fireEvent.change(purposeInput, { target: { value: 'My conflicting purpose edit' } });

    // Submit
    fireEvent.click(screen.getByTestId('save-software-button'));

    // Conflict banner appears
    const conflictBanner = await screen.findByTestId('mutation-conflict-banner');
    expect(conflictBanner).toHaveTextContent('stale_revision');
    expect(conflictBanner).toHaveTextContent('stale revision: expected 1, actual 2');

    // Form is NOT submitted again automatically (no auto-retry)
    expect(fakeTransport.softwareCommand).toHaveBeenCalledTimes(1);

    // Draft is preserved
    expect(purposeInput.value).toBe('My conflicting purpose edit');

    // User can manually discard draft and reload latest
    const reloadBtn = screen.getByTestId('reload-latest-button');
    fireEvent.click(reloadBtn);

    await waitFor(() => {
      expect(screen.queryByTestId('mutation-conflict-banner')).not.toBeInTheDocument();
      expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
    });
  });

  it('Path 4 (Transaction Failure): internal error banner displayed -> draft preserved', async () => {
    vi.spyOn(fakeTransport, 'softwareCommand').mockRejectedValueOnce({
      category: 'internal_error',
      message: 'database locked or disk I/O error occurred',
    });

    render(<AssetDetailView assetId="asset-sw-001" />);

    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-software-button'));

    const purposeInput = screen.getByTestId('software-purpose-input') as HTMLInputElement;
    fireEvent.change(purposeInput, { target: { value: 'New purpose attempt' } });

    fireEvent.click(screen.getByTestId('save-software-button'));

    const errorBanner = await screen.findByTestId('mutation-error-banner');
    expect(errorBanner).toHaveTextContent('database locked or disk I/O error occurred');

    // Draft is preserved
    expect(purposeInput.value).toBe('New purpose attempt');
    expect(screen.getByTestId('software-edit-form')).toBeInTheDocument();
  });

  it('Path 5 (No-Op): identical input returns changed: false -> UI displays no changes detected', async () => {
    const noOpReceipt: MutationReceiptDto = {
      operation: 'software.update_metadata',
      asset_ids: ['asset-sw-001'],
      revision: 1,
      changed: false,
      warnings: ['No-op: no fields were updated'],
    };
    vi.spyOn(fakeTransport, 'softwareCommand').mockResolvedValueOnce(noOpReceipt);

    render(<AssetDetailView assetId="asset-sw-001" />);

    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-software-button'));

    // Submit without modifications
    fireEvent.click(screen.getByTestId('save-software-button'));

    await waitFor(() => {
      expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
      expect(screen.getByTestId('mutation-receipt-badge')).toHaveTextContent('No changes detected');
    });
  });
});
