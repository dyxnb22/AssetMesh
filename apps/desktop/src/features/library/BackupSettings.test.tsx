import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { BackupSettings } from './BackupSettings';
import { InfoPanel } from './InfoEditor';
import { setTransport } from './transport';
import type { AssetDetailDto, InfoRecordDto } from './types';

describe('personal backups and recovery', () => {
  let transport: FakeDesktopTransport;
  beforeEach(() => {
    localStorage.clear();
    transport = new FakeDesktopTransport();
    setTransport(transport);
  });

  it('reports affected backups while still allowing preview of an available backup', async () => {
    await transport.backupCreate();
    const base = await transport.backupStatus();
    vi.spyOn(transport, 'backupStatus').mockResolvedValue({ ...base,
      issues: [{ source_dir: '/backups/damaged', message: 'Invalid manifest' }] });
    render(<BackupSettings />);
    await screen.findByText(/1 backups could not be listed/);
    fireEvent.click(screen.getByText('Restore backup', { selector: 'summary' }));
    fireEvent.click(screen.getByRole('button', { name: 'Preview restore' }));
    await screen.findByTestId('backup-restore-preview');
    expect(screen.getByRole('button', { name: 'Restore into a new library' })).toBeDisabled();
  });

  it('creates a backup with preferences and restores only after preview and confirmation', async () => {
    localStorage.setItem('assetmesh-theme', 'dark');
    const save = vi.spyOn(transport, 'backupSavePreferences');
    const restore = vi.spyOn(transport, 'backupRestore');
    render(<BackupSettings />);
    await screen.findByText(/No backup yet/);
    expect(screen.getByRole('button', { name: 'Choose backup to restore' })).not.toBeVisible();
    expect(screen.getByRole('button', { name: 'Back up now' })).not.toBeVisible();
    fireEvent.click(screen.getByText('Backup details', { selector: 'summary' }));
    fireEvent.click(screen.getByRole('button', { name: 'Back up now' }));
    await screen.findByText('Backup completed.');
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ 'assetmesh-theme': 'dark' }));
    fireEvent.click(screen.getByText('Restore backup', { selector: 'summary' }));
    fireEvent.click(screen.getByRole('button', { name: 'Preview restore' }));
    await screen.findByTestId('backup-restore-preview');
    const apply = screen.getByRole('button', { name: 'Restore into a new library' });
    expect(apply).toBeDisabled();
    expect(restore).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('checkbox'));
    fireEvent.click(apply);
    await screen.findByText(/Recovery is ready\. Quit and reopen/);
    expect(restore).toHaveBeenCalledWith('/test/assetmesh.backups/backup-1', 'backup-1');
    expect(screen.getByRole('button', { name: 'Back up now' })).toBeDisabled();
  });

  it('rejects a bad source without leaving a usable restore confirmation', async () => {
    await transport.backupCreate();
    render(<BackupSettings />);
    fireEvent.click(screen.getByText('Restore backup', { selector: 'summary' }));
    await screen.findByRole('button', { name: 'Preview restore' });
    fireEvent.click(screen.getByRole('button', { name: 'Preview restore' }));
    await screen.findByTestId('backup-restore-preview');
    fireEvent.click(screen.getByRole('checkbox'));
    transport.nextPickedDirectory = '/damaged-backup';
    fireEvent.click(screen.getByRole('button', { name: 'Choose backup to restore' }));
    await screen.findByText('Backup not found');
    expect(screen.queryByTestId('backup-restore-preview')).not.toBeInTheDocument();
  });

  it('keeps recovery accessible when the current library failed to open', async () => {
    await transport.backupCreate();
    render(<BackupSettings recoveryOnly />);
    await screen.findByRole('button', { name: 'Preview restore' });
    expect(screen.queryByRole('button', { name: 'Back up now' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Preview restore' }));
    await screen.findByTestId('backup-restore-preview');
  });

  it('saves an external recovery copy and cancellation writes nothing', async () => {
    const exportCopy = vi.spyOn(transport, 'backupExportCopy');
    transport.nextPickedDirectory = null;
    render(<BackupSettings />);
    await screen.findByText(/No backup yet/);
    fireEvent.click(screen.getByRole('button', { name: 'Save a backup copy…' }));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Save a backup copy…' })).not.toBeDisabled());
    expect(exportCopy).not.toHaveBeenCalled();
    transport.nextPickedDirectory = '/external';
    fireEvent.click(screen.getByRole('button', { name: 'Save a backup copy…' }));
    await screen.findByText('Backup copy saved to /external/assetmesh-backup-copy');
    expect(exportCopy).toHaveBeenCalledWith('/external');
  });

  it('surfaces automatic backup failures and sensitive backup contents', async () => {
    const entry = await transport.backupCreate();
    vi.spyOn(transport, 'backupStatus').mockResolvedValue({ directory: '/backups', entries: [{ ...entry, contains_api_keys: true }], issues: [], storage_bytes: 1_048_576, budget_bytes: 268_435_456, last_error: 'Disk full', restore_pending: false });
    render(<BackupSettings />);
    await screen.findByText(/Disk full/);
    expect(screen.getByTestId('backup-sensitive-notice')).toHaveTextContent('not encrypted');
  });
});

describe('API key visibility', () => {
  it('starts masked, permits explicit reveal, and masks when the asset changes', () => {
    const record: InfoRecordDto = { module: 'info', asset_id: 'key-1', info_type: 'api_key', value: 'private-value-123', notes: null };
    const detail: AssetDetailDto = { id: 'key-1', kind: 'info.item', name: 'Key', summary: null, lifecycle: 'active', revision: 1, created_at: '', updated_at: '', archived_at: null, merged_into: null, details: record, tags: [], external_refs: [] };
    const view = render(<InfoPanel detail={detail} record={record} onSaved={vi.fn()} />);
    expect(screen.getByTestId('info-value')).not.toHaveTextContent(record.value);
    fireEvent.click(screen.getByRole('button', { name: 'Show value' }));
    expect(screen.getByTestId('info-value')).toHaveTextContent(record.value);
    view.rerender(<InfoPanel detail={{ ...detail, id: 'key-2' }} record={{ ...record, asset_id: 'key-2', value: 'another-value' }} onSaved={vi.fn()} />);
    expect(screen.getByTestId('info-value')).not.toHaveTextContent('another-value');
  });
});
