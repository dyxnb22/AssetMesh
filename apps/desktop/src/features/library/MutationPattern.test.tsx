import { act, render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary, MutationReceiptDto } from './types';

const software: AssetSummary = {
  id: 'asset-sw-001', kind: 'software.tool', name: 'ripgrep', subtitle: 'Search tool', lifecycle: 'active',
  tags: ['cli'], updated_at: '2024-03-20T10:00:00Z',
};

async function editPurpose(value: string) {
  render(<AssetDetailView assetId={software.id} />);
  await screen.findByTestId('asset-detail-view');
  fireEvent.click(screen.getByTestId('edit-software-button'));
  const input = screen.getByTestId('software-purpose-input');
  fireEvent.change(input, { target: { value } });
  await act(async () => { fireEvent.click(screen.getByTestId('save-software-button')); });
  return input;
}

describe('Mutation failures and unchanged receipts', () => {
  let fake: FakeDesktopTransport;
  beforeEach(() => { fake = new FakeDesktopTransport([software]); setTransport(fake); });

  // The successful save/read-back workflow lives in SoftwareWorkflow.
  it.each([
    { category: 'invalid_input', message: 'Purpose is invalid.', banner: 'mutation-validation-banner' },
    { category: 'unavailable', message: 'Storage is busy; retry later.', banner: 'mutation-error-banner' },
  ])('keeps the editable draft after $category and retries only on request', async ({ category, message, banner }) => {
    const command = vi.spyOn(fake, 'softwareCommand').mockRejectedValueOnce({ category, message });
    const input = await editPurpose('Unsaved purpose');
    expect(await screen.findByTestId(banner)).toHaveTextContent(message);
    expect(input).toHaveValue('Unsaved purpose');
    expect(screen.getByTestId('software-edit-form')).toBeInTheDocument();
    expect(command).toHaveBeenCalledTimes(1); // A failed mutation must not repeat automatically.
    fireEvent.click(screen.getByTestId('save-software-button'));
    await screen.findByText('Unsaved purpose');
    expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
  });

  it('preserves a conflicting draft until the user chooses to reload', async () => {
    const command = vi.spyOn(fake, 'softwareCommand').mockRejectedValueOnce({ category: 'stale_revision', message: 'This record changed elsewhere.' });
    const input = await editPurpose('My conflicting edit');
    expect(await screen.findByTestId('mutation-conflict-banner')).toHaveTextContent('This record changed elsewhere.');
    expect(input).toHaveValue('My conflicting edit');
    expect(command).toHaveBeenCalledTimes(1);
    await act(async () => { fireEvent.click(screen.getByTestId('reload-latest-button')); });
    await waitFor(() => expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument());
    expect(screen.queryByTestId('mutation-conflict-banner')).not.toBeInTheDocument();
    expect(screen.getByText('Productivity tool')).toBeInTheDocument();
  });

  it('keeps the form open when the command succeeds but canonical read-back fails', async () => {
    const onUpdated = vi.fn();
    render(<AssetDetailView assetId={software.id} onAssetUpdated={onUpdated} />);
    await screen.findByTestId('asset-detail-view');
    vi.spyOn(fake, 'getAsset').mockRejectedValueOnce({ category: 'unavailable', message: 'Read-back failed.' });
    fireEvent.click(screen.getByTestId('edit-software-button'));
    fireEvent.change(screen.getByTestId('software-purpose-input'), { target: { value: 'Keep this draft' } });
    await act(async () => { fireEvent.click(screen.getByTestId('save-software-button')); });
    expect(await screen.findByTestId('mutation-error-banner')).toHaveTextContent('Read-back failed.');
    expect(screen.getByTestId('software-purpose-input')).toHaveValue('Keep this draft');
    expect(screen.getByTestId('save-software-button')).toBeEnabled();
    expect(screen.queryByTestId('mutation-receipt-badge')).not.toBeInTheDocument();
    expect(onUpdated).not.toHaveBeenCalled();
  });

  it('presents an unchanged receipt without claiming that a save changed data', async () => {
    vi.spyOn(fake, 'softwareCommand').mockResolvedValueOnce({ operation: 'software.update_metadata', asset_ids: [software.id], revision: 1, changed: false, warnings: [] });
    render(<AssetDetailView assetId={software.id} />);
    await screen.findByTestId('asset-detail-view');
    fireEvent.click(screen.getByTestId('edit-software-button'));
    fireEvent.click(screen.getByTestId('save-software-button'));
    await waitFor(() => expect(screen.getByTestId('mutation-receipt-badge')).toHaveTextContent('No changes detected'));
    expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
  });

  it.each([
    { open: 'edit-service-button', form: 'service-edit-form', submit: 'save-service-button', input: 'service-plan-input' },
    { open: 'record-renewal-button', form: 'record-renewal-form', submit: 'submit-renewal-button', input: 'renewal-cost-input' },
  ])('discards the $form draft when reloading a conflicting service', async ({ open, form, submit, input }) => {
    const service = { ...software, id: 'service-conflict', kind: 'service.saas', name: 'Subscription' };
    fake = new FakeDesktopTransport([service]);
    setTransport(fake);
    const canonical = await fake.getAsset(service.id);
    vi.spyOn(fake, 'serviceCommand').mockRejectedValueOnce({ category: 'stale_revision', message: 'Changed elsewhere.' });
    render(<AssetDetailView assetId={service.id} />);
    fireEvent.click(await screen.findByTestId(open));
    fireEvent.change(screen.getByTestId(input), { target: { value: '42.00' } });
    fireEvent.click(screen.getByTestId(submit));
    await screen.findByTestId('mutation-conflict-banner');
    expect(screen.getByTestId(input)).toHaveValue('42.00');
    vi.spyOn(fake, 'getAsset').mockResolvedValueOnce({ ...canonical, name: 'Canonical subscription', revision: canonical.revision + 1 });
    fireEvent.click(screen.getByTestId('reload-latest-button'));
    await screen.findByRole('heading', { name: 'Canonical subscription' });
    expect(screen.queryByTestId(form)).not.toBeInTheDocument();
    expect(screen.queryByTestId('mutation-conflict-banner')).not.toBeInTheDocument();
  });

  it('keeps the selected detail when an earlier asset finishes saving', async () => {
    const other = { ...software, id: 'another-software', name: 'Other application' };
    fake = new FakeDesktopTransport([software, other]);
    setTransport(fake);
    let finish!: (receipt: MutationReceiptDto) => void;
    vi.spyOn(fake, 'softwareCommand').mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    const onUpdated = vi.fn();
    const view = render(<AssetDetailView assetId={software.id} onAssetUpdated={onUpdated} />);
    fireEvent.click(await screen.findByTestId('edit-software-button'));
    fireEvent.click(screen.getByTestId('save-software-button'));
    view.rerender(<AssetDetailView assetId={other.id} onAssetUpdated={onUpdated} />);
    await screen.findByRole('heading', { name: other.name });
    const receipt = { operation: 'software.update_metadata', asset_ids: [software.id], revision: 2, changed: true, warnings: [] };
    await act(async () => { finish(receipt); });
    expect(screen.getByRole('heading', { name: other.name })).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: software.name })).not.toBeInTheDocument();
    expect(onUpdated).toHaveBeenCalledWith(receipt);
  });

  it('blocks another submit while a detail command is awaiting read-back', async () => {
    let finish!: (receipt: MutationReceiptDto) => void;
    const command = vi.spyOn(fake, 'softwareCommand').mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
    render(<AssetDetailView assetId={software.id} />);
    fireEvent.click(await screen.findByTestId('edit-software-button'));
    const form = screen.getByTestId('software-edit-form');
    act(() => { fireEvent.submit(form); fireEvent.submit(form); });
    expect(command).toHaveBeenCalledTimes(1);
    await act(async () => { finish({ operation: 'software.update_metadata', asset_ids: [software.id], revision: 2, changed: true, warnings: [] }); });
    expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
  });

  it('closes editable drafts when the asset is archived', async () => {
    render(<AssetDetailView assetId={software.id} />);
    fireEvent.click(await screen.findByTestId('edit-software-button'));
    fireEvent.click(screen.getByTestId('archive-asset-button'));
    fireEvent.click(screen.getByTestId('confirm-archive-button'));
    await screen.findByText('Archived Asset:');
    expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
    expect(screen.queryByTestId('edit-software-button')).not.toBeInTheDocument();
  });
});
