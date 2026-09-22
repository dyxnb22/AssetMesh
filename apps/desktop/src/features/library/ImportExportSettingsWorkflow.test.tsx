import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { App } from '../../app/App';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { ImportExportView } from './ImportExportView';
import { SettingsView } from './SettingsView';
import { setTransport } from './transport';

describe('Import/Export and Settings Workflow UI (P5-09)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport();
    setTransport(fakeTransport);
  });

  it('Flow 1 (Export Bundle): executes export, displays manifest summary and completion receipt', async () => {
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

  it('Flow 3 (Import Preflight Preview): runs preflight, renders dispositions table, blocks apply without confirmation', async () => {
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
    const table = screen.getByTestId('import-dispositions-table');
    expect(table).toBeInTheDocument();
    expect(screen.getByText('+3')).toBeInTheDocument(); // Assets created

    // Apply button exists but is disabled until confirmation checkbox is checked
    const applyBtn = screen.getByTestId('apply-import-button');
    expect(applyBtn).toBeDisabled();

    const checkbox = screen.getByTestId('import-confirm-checkbox');
    expect(checkbox).not.toBeChecked();

    // Check confirmation checkbox -> Apply button becomes enabled
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();
    expect(applyBtn).not.toBeDisabled();
  });

  it('Flow 4 (Explicit Apply & Dry-Run Symmetry): executes apply and displays completed receipt matching preflight report', async () => {
    render(<ImportExportView />);

    fireEvent.click(screen.getByTestId('tab-import'));

    const dirInput = screen.getByTestId('import-dir-input');
    fireEvent.change(dirInput, { target: { value: '/path/to/valid-bundle' } });
    fireEvent.click(screen.getByTestId('preview-import-button'));

    await screen.findByTestId('import-preview-container');

    // Confirm and apply
    fireEvent.click(screen.getByTestId('import-confirm-checkbox'));
    const applyBtn = screen.getByTestId('apply-import-button');
    fireEvent.click(applyBtn);

    // Completion receipt appears
    const receipt = await screen.findByTestId('import-receipt');
    expect(receipt).toBeInTheDocument();
    expect(screen.getByText('✓ Import Successfully Applied')).toBeInTheDocument();
    expect(screen.getByText(/Assets: \+3 created, 0 updated/)).toBeInTheDocument();
    expect(screen.getByText(/Media: \+1 created/)).toBeInTheDocument();
  });

  it('Flow 5 (Malformed Bundle Rejection): preflight displays error banner, apply button is blocked', async () => {
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
    render(<ImportExportView />);

    fireEvent.click(screen.getByTestId('tab-import'));

    const dirInput = screen.getByTestId('import-dir-input');
    // 'collision' triggers conflict error
    fireEvent.change(dirInput, { target: { value: '/path/to/collision-bundle' } });
    fireEvent.click(screen.getByTestId('preview-import-button'));

    await screen.findByTestId('import-error-banner');
    expect(screen.getByText(/Collision: external ref imdb:tt0000001/)).toBeInTheDocument();
    expect(screen.queryByTestId('apply-import-button')).not.toBeInTheDocument();
  });

  it('Flow 7 (Settings View): displays real DB location and status, providers, and supports theme switching', async () => {
    let currentTheme: 'system' | 'light' | 'dark' = 'system';
    const handleThemeChange = (newTheme: 'system' | 'light' | 'dark') => {
      currentTheme = newTheme;
    };

    const { rerender } = render(
      <SettingsView currentTheme={currentTheme} onThemeChange={handleThemeChange} />
    );

    // Database section renders with real path and status
    await screen.findByTestId('settings-db-section');
    expect(screen.getByTestId('settings-db-path')).toHaveTextContent(/assetmesh\.db/);
    expect(screen.getByTestId('settings-db-status')).toHaveTextContent('Ready');

    // Discovery providers are listed
    expect(screen.getByTestId('provider-card-macos_applications')).toBeInTheDocument();
    expect(screen.getByTestId('provider-card-homebrew')).toBeInTheDocument();
    expect(screen.getByTestId('provider-card-cli_tools')).toBeInTheDocument();
    expect(screen.getAllByTestId('provider-status-available').length).toBe(3);

    // Capabilities section renders
    expect(screen.getByTestId('settings-capabilities-section')).toBeInTheDocument();
    expect(screen.getByText('media, software, services')).toBeInTheDocument();

    // Theme switching: click Dark
    const darkBtn = screen.getByTestId('theme-dark');
    fireEvent.click(darkBtn);
    expect(currentTheme).toBe('dark');

    // Rerender with updated theme
    rerender(<SettingsView currentTheme={currentTheme} onThemeChange={handleThemeChange} />);
    expect(screen.getByTestId('theme-dark')).toHaveStyle({ fontWeight: 600 });
  });

  it('Flow 8 (Navigation Rail - Workspaces): navigates to Portable Data and Settings sections', async () => {
    render(<App />);

    // Click Portable Data tab in navigation rail
    const importExportTab = await screen.findByTestId('nav-import-export');
    fireEvent.click(importExportTab);

    await screen.findByTestId('import-export-workspace');
    expect(screen.getByText('Portable Data (Import / Export)')).toBeInTheDocument();

    // Click Settings tab in navigation rail
    const settingsTab = screen.getByTestId('nav-settings');
    fireEvent.click(settingsTab);

    await screen.findByTestId('settings-workspace');
    expect(screen.getByText('Settings & Environment')).toBeInTheDocument();
    expect(screen.getByText('Local Database')).toBeInTheDocument();
  });
});
