import { EMPTY_IMPORT_REPORT, importPreviewFixture } from '../../test/query-fixtures';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from '../../app/App';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { ImportExportView } from './ImportExportView';
import { setTransport } from './transport';

describe('Import/Export and Settings Workflow UI (P5-09)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport();
    setTransport(fakeTransport);
  });

  it('Flow 1 (Export Bundle): executes export, displays manifest summary and completion receipt', async () => {
    const exportCall = vi.spyOn(fakeTransport, 'portableExport');
    render(<ImportExportView />);

    // Initially on Export tab
    expect(screen.getByText('Export Portable Bundle')).toBeInTheDocument();

    const dirInput = screen.getByTestId('export-dir-input');
    const exportBtn = screen.getByTestId('execute-export-button');

    // Button disabled when empty
    expect(exportBtn).toBeDisabled();

    // Type destination directory
    fireEvent.change(dirInput, { target: { value: '/tmp/test-export-dir' } });
    expect(exportBtn).not.toBeDisabled();

    // Execute export
    fireEvent.click(exportBtn);

    // Export receipt appears
    const receipt = await screen.findByTestId('export-receipt');
    expect(exportCall).toHaveBeenCalledWith('/tmp/test-export-dir', false);
    expect(receipt).toBeInTheDocument();
    expect(screen.getByText(/✓ Export Complete/)).toBeInTheDocument();
    expect(screen.getByText(/Receipt: \/tmp\/test-export-dir/)).toBeInTheDocument();
    expect(screen.getByText(/assetmesh-portable-export \(v1\)/)).toBeInTheDocument();

    // Files list exists
    const filesList = screen.getByTestId('export-files-list');
    expect(filesList).toBeInTheDocument();
    expect(screen.getByText('manifest.json')).toBeInTheDocument();
    expect(screen.getByText('assets.jsonl')).toBeInTheDocument();
    expect(screen.getByText('modules/media.jsonl')).toBeInTheDocument();
  });
  it('preserves both workflows and the inspected source when switching tabs', async () => {
    const apply = vi.spyOn(fakeTransport, 'portableImportApply');
    render(<ImportExportView />);
    fireEvent.change(screen.getByTestId('export-dir-input'), { target: { value: '/tmp/export-draft' } });
    fireEvent.click(screen.getByRole('checkbox', { name: 'Include API key values (not encrypted)' }));
    fireEvent.click(screen.getByTestId('tab-import'));
    fireEvent.change(screen.getByTestId('import-dir-input'), { target: { value: '/tmp/import-draft' } });
    fireEvent.click(screen.getByTestId('preview-import-button'));
    await screen.findByTestId('import-preview-container');

    fireEvent.click(screen.getByTestId('tab-export'));
    expect(screen.getByTestId('export-dir-input')).toHaveValue('/tmp/export-draft');
    expect(screen.getByRole('checkbox', { name: 'Include API key values (not encrypted)' })).toBeChecked();
    fireEvent.click(screen.getByTestId('tab-import'));
    expect(screen.getByTestId('import-dir-input')).toHaveValue('/tmp/import-draft');
    expect(screen.getByTestId('apply-import-button')).not.toBeDisabled();
    fireEvent.click(screen.getByTestId('apply-import-button'));
    await screen.findByTestId('import-receipt');
    expect(apply).toHaveBeenCalledWith('/tmp/import-draft', importPreviewFixture('/tmp/import-draft').fingerprint);
  });

  it('includes API keys only after an explicit choice and explains storage protection', async () => {
    const exportCall = vi.spyOn(fakeTransport, 'portableExport');
    render(<ImportExportView />);
    const keys = screen.getByRole('checkbox', { name: 'Include API key values (not encrypted)' });
    expect(keys).not.toBeChecked();
    fireEvent.click(keys);
    expect(screen.getByText('Keep exports containing API keys on encrypted storage.')).toBeInTheDocument();
    fireEvent.change(screen.getByTestId('export-dir-input'), { target: { value: '/tmp/private-export' } });
    fireEvent.click(screen.getByTestId('execute-export-button'));
    await screen.findByTestId('export-receipt');
    expect(exportCall).toHaveBeenCalledWith('/tmp/private-export', true);
  });

  it('Flow 2 (Directory Picker & Cancellation): browse updates path, cancellation leaves path unchanged', async () => {
    render(<ImportExportView />);

    const dirInput = screen.getByTestId('export-dir-input') as HTMLInputElement;
    const browseBtn = screen.getByTestId('export-browse-button');

    // Case A: Picker selects a directory
    fakeTransport.nextPickedDirectory = '/Users/test/picked-export';
    fireEvent.click(browseBtn);

    // Input should be populated
    await screen.findByDisplayValue('/Users/test/picked-export');
    expect(dirInput.value).toBe('/Users/test/picked-export');

    // Case B: User opens picker and cancels (returns null)
    fakeTransport.nextPickedDirectory = null;
    fireEvent.click(browseBtn);

    // Path must NOT be cleared or changed on cancellation
    expect(dirInput.value).toBe('/Users/test/picked-export');
  });

  it('Flow 3 (Import Preflight Preview): runs preflight, renders dispositions table, offers one explicit apply action', async () => {
    render(<ImportExportView />);

    // Switch to Import tab
    const importTab = screen.getByTestId('tab-import');
    fireEvent.click(importTab);

    expect(screen.getByText('Import Portable Bundle')).toBeInTheDocument();

    const dirInput = screen.getByTestId('import-dir-input');
    const previewBtn = screen.getByTestId('preview-import-button');

    fireEvent.change(dirInput, { target: { value: '/path/to/valid-bundle' } });
    fireEvent.click(previewBtn);

    // Preflight container appears
    await screen.findByTestId('import-preview-container');
    expect(screen.getByText('✓ Preflight Passed')).toBeInTheDocument();

    // Dispositions table renders with predicted counts
    fireEvent.click(screen.getByText('Technical details', { selector: 'summary' }));
    const table = screen.getByTestId('import-dispositions-table');
    expect(table).toBeInTheDocument();
    expect(screen.getByText('+3')).toBeInTheDocument(); // Assets created

    // The reviewed summary leads to one explicit apply action.
    expect(screen.getByTestId('apply-import-button')).not.toBeDisabled();
    expect(screen.queryByTestId('import-confirm-checkbox')).not.toBeInTheDocument();
  });

  it('applies the previewed path and fingerprint, then presents its receipt', async () => {
    const apply = vi.spyOn(fakeTransport, 'portableImportApply');
    render(<ImportExportView />);

    fireEvent.click(screen.getByTestId('tab-import'));

    const dirInput = screen.getByTestId('import-dir-input');
    fireEvent.change(dirInput, { target: { value: '/path/to/valid-bundle' } });
    fireEvent.click(screen.getByTestId('preview-import-button'));

    await screen.findByTestId('import-preview-container');

    // Confirm and apply
    const applyBtn = screen.getByTestId('apply-import-button');
    fireEvent.click(applyBtn);

    // Completion receipt appears
    const receipt = await screen.findByTestId('import-receipt');
    expect(receipt).toBeInTheDocument();
    expect(apply).toHaveBeenCalledWith('/path/to/valid-bundle', importPreviewFixture('/path/to/valid-bundle').fingerprint);
    expect(screen.getByText('✓ Import Successfully Applied')).toBeInTheDocument();
    expect(screen.getByText(/Assets: \+3 created, 0 updated/)).toBeInTheDocument();
    expect(screen.getByText(/Media: \+1 created/)).toBeInTheDocument();
  });

  it('Flow 5 (Malformed Bundle Rejection): preflight displays error banner, apply button is blocked', async () => {
    fakeTransport.nextImportPreview = importPreviewFixture('/path/to/malformed-bundle', { valid: false, dispositions: { ...EMPTY_IMPORT_REPORT }, errors: ['Failed to read bundle: invalid json syntax in assets.jsonl'] });

    render(<ImportExportView />);

    fireEvent.click(screen.getByTestId('tab-import'));

    const dirInput = screen.getByTestId('import-dir-input');
    // 'malformed' in path triggers simulated malformed bundle
    fireEvent.change(dirInput, { target: { value: '/path/to/malformed-bundle' } });
    fireEvent.click(screen.getByTestId('preview-import-button'));

    // Preflight fails loudly
    await screen.findByTestId('import-error-banner');
    expect(screen.getByText(/Preflight Rejection:/)).toBeInTheDocument();
    expect(screen.getByText(/invalid json syntax in assets.jsonl/)).toBeInTheDocument();

    // Apply button is not rendered or blocked
    expect(screen.queryByTestId('apply-import-button')).not.toBeInTheDocument();
  });

  it('Flow 6 (Collision / Conflict Rejection): preflight identifies conflict, blocks apply', async () => {
    fakeTransport.nextImportPreview = importPreviewFixture('/path/to/collision-bundle', { valid: false, dispositions: { ...EMPTY_IMPORT_REPORT }, errors: ['Collision: external ref imdb:tt0000001 is already attached to another asset'] });

    render(<ImportExportView />);

    fireEvent.click(screen.getByTestId('tab-import'));

    const dirInput = screen.getByTestId('import-dir-input');
    // The configured collision is presented without executing an import
    fireEvent.change(dirInput, { target: { value: '/path/to/collision-bundle' } });
    fireEvent.click(screen.getByTestId('preview-import-button'));

    await screen.findByTestId('import-error-banner');
    expect(screen.getByText(/Collision: external ref imdb:tt0000001/)).toBeInTheDocument();
    expect(screen.queryByTestId('apply-import-button')).not.toBeInTheDocument();
  });

  it('Flow 8 (Navigation Rail - Workspaces): navigates to Portable Data and Settings sections', async () => {
    render(<App />);

    await screen.findByTestId('nav-all');
    fireEvent.click(screen.getByText('Tools', { selector: 'summary' }));

    // Click Portable Data tab in navigation rail
    const importExportTab = await screen.findByTestId('nav-import-export');
    fireEvent.click(importExportTab);

    await screen.findByTestId('import-export-workspace');

    // Click Settings tab in navigation rail
    const settingsTab = screen.getByTestId('nav-settings');
    fireEvent.click(settingsTab);

    await screen.findByTestId('settings-workspace');
    expect(screen.getByTestId('theme-system')).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByTestId('settings-language-section')).toBeVisible();
    expect(screen.getByRole('button', { name: 'Save a backup copy…' })).toBeVisible();
  });

  // -------------------------------------------------------------------------
  // Preview/apply path binding
  // -------------------------------------------------------------------------

  it('applies the bundle the preflight inspected, not whatever the field says now', async () => {
    // The apply used to send the live input, so a backend that resolved or
    // normalized the path imported something other than what it had inspected.
    // Simulated here: the preview reports a different `source_dir` than the
    // path typed, which is what a resolving backend does.
    fakeTransport.nextImportPreview = importPreviewFixture('/resolved/by/backend');

    render(<ImportExportView />);
    fireEvent.click(screen.getByTestId('tab-import'));

    fireEvent.change(screen.getByTestId('import-dir-input'), {
      target: { value: '/typed/path' },
    });
    fireEvent.click(screen.getByTestId('preview-import-button'));
    await screen.findByTestId('import-preview-container');

    fireEvent.click(screen.getByTestId('apply-import-button'));
    await screen.findByTestId('import-receipt');

    // Not the typed path — the one the backend says it read.
    expect(fakeTransport.importApplyCalls).toEqual(['/resolved/by/backend']);
  });

  it('refuses to apply once the source field no longer matches the preview', async () => {
    render(<ImportExportView />);
    fireEvent.click(screen.getByTestId('tab-import'));

    fireEvent.change(screen.getByTestId('import-dir-input'), {
      target: { value: '/path/to/valid-bundle' },
    });
    fireEvent.click(screen.getByTestId('preview-import-button'));
    await screen.findByTestId('import-preview-container');

    // Redirect the field to a different bundle after confirming. The checkbox
    // on screen is still checked, but it no longer describes what would run.
    fireEvent.change(screen.getByTestId('import-dir-input'), {
      target: { value: '/path/to/other-bundle' },
    });

    const applyBtn = screen.getByTestId('apply-import-button');
    expect(applyBtn).toBeDisabled();
    expect(screen.getByTestId('import-stale-notice')).toBeInTheDocument();
    expect(fakeTransport.importApplyCalls).toEqual([]);

    // Re-running the preview for the new path clears the staleness.
    fireEvent.click(screen.getByTestId('preview-import-button'));
    await waitFor(() => {
      expect(screen.queryByTestId('import-stale-notice')).not.toBeInTheDocument();
    });
    expect(applyBtn).not.toBeDisabled(); // fresh preview enables the explicit apply action
  });

  it('drops the import receipt when the source field changes', async () => {
    // A receipt belongs to a bundle that has already been applied; leaving it
    // attached to a directory the user has since moved off is misleading.
    render(<ImportExportView />);
    fireEvent.click(screen.getByTestId('tab-import'));

    fireEvent.change(screen.getByTestId('import-dir-input'), {
      target: { value: '/path/to/valid-bundle' },
    });
    fireEvent.click(screen.getByTestId('preview-import-button'));
    await screen.findByTestId('import-preview-container');
    fireEvent.click(screen.getByTestId('apply-import-button'));
    await screen.findByTestId('import-receipt');

    fireEvent.change(screen.getByTestId('import-dir-input'), {
      target: { value: '/path/to/valid-bundle-2' },
    });
    expect(screen.queryByTestId('import-receipt')).not.toBeInTheDocument();
  });
});
