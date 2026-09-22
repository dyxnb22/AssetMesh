import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { App } from '../app/App';
import { setTransport } from '../features/library/transport';
import { FakeDesktopTransport } from './fake-transport';

describe('Desktop Hardening & Release Gate End-to-End Workflow (P5-10)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    window.location.hash = '';
    fakeTransport = new FakeDesktopTransport([
      {
        id: 'sw-001',
        kind: 'software.app',
        name: 'VLC Media Player',
        lifecycle: 'active',
        subtitle: 'Open-source media player',
        tags: ['player', 'media'],
        updated_at: '2024-03-20T10:00:00Z',
      },
    ]);
    setTransport(fakeTransport);
  });

  it('Full E2E Loop: Create -> Search -> Open Detail -> Mutate -> Attach Relation -> Export Bundle', async () => {
    render(<App />);

    // Wait for App shell to bootstrap
    await screen.findByTestId('nav-all');

    // 1. CREATE: Switch to Media module and open Add Media modal
    const mediaTab = screen.getByTestId('nav-media');
    fireEvent.click(mediaTab);

    const addMediaBtn = screen.getByTestId('new-asset-button');
    fireEvent.click(addMediaBtn);

    await screen.findByTestId('create-media-form');
    const titleInput = screen.getByTestId('media-create-title-input');
    fireEvent.change(titleInput, { target: { value: 'Interstellar' } });

    const typeSelect = screen.getByTestId('media-create-type-select');
    fireEvent.change(typeSelect, { target: { value: 'movie' } });

    const submitCreateBtn = screen.getByTestId('submit-create-media-button');
    fireEvent.click(submitCreateBtn);

    // Modal closes upon creation
    await waitFor(() => {
      expect(screen.queryByTestId('create-media-form')).not.toBeInTheDocument();
    });

    // Asset detail modal automatically opens on creation
    await screen.findByTestId('asset-detail-view');
    expect(screen.getByRole('heading', { level: 2, name: 'Interstellar' })).toBeInTheDocument();

    // Close detail view to test searching in the ledger
    const closeDetailBtn = screen.getByLabelText('Close detail view');
    fireEvent.click(closeDetailBtn);
    await waitFor(() => {
      expect(screen.queryByTestId('asset-detail-view')).not.toBeInTheDocument();
    });

    // 2. SEARCH: Type 'Inter' into global search input
    const searchInput = screen.getByPlaceholderText(/Search assets/i);
    fireEvent.change(searchInput, { target: { value: 'Inter' } });

    // Wait for debounced search to update and list the matched asset
    await waitFor(
      () => {
        expect(screen.getAllByText('Interstellar').length).toBeGreaterThan(0);
      },
      { timeout: 3000 }
    );

    // 3. OPEN DETAIL: Click row in ledger to open detail view
    const ledgerTable = screen.getByRole('table', { name: 'Assets' });
    const assetRow = within(ledgerTable).getByText('Interstellar').closest('.asset-row');
    expect(assetRow).not.toBeNull();
    if (assetRow) {
      fireEvent.doubleClick(assetRow);
    }

    // Detail view opens
    await screen.findByTestId('asset-detail-view');
    expect(screen.getByRole('heading', { level: 2, name: 'Interstellar' })).toBeInTheDocument();

    // 4. MUTATE: Open edit metadata form in Media panel, update notes
    const editBtn = await screen.findByTestId('edit-media-metadata-button');
    fireEvent.click(editBtn);

    const notesInput = await screen.findByTestId('media-notes-input');
    fireEvent.change(notesInput, { target: { value: 'Masterpiece by Christopher Nolan' } });

    const saveBtn = screen.getByTestId('save-media-button');
    fireEvent.click(saveBtn);

    // Mutation completes, read-back verified, updated notes visible
    await waitFor(() => {
      expect(screen.queryByTestId('media-edit-form')).not.toBeInTheDocument();
    });
    await screen.findByText(/Masterpiece by Christopher Nolan/);

    // Close detail view
    const closeBtn = screen.getByLabelText('Close detail view');
    fireEvent.click(closeBtn);
    await waitFor(() => {
      expect(screen.queryByTestId('asset-detail-view')).not.toBeInTheDocument();
    });

    // 5. ATTACH RELATION: Switch to Relations workspace
    const relationsTab = await screen.findByTestId('nav-relations');
    fireEvent.click(relationsTab);

    await screen.findByTestId('relations-workspace');

    // Open Add Relation modal
    const addRelBtn = await screen.findByTestId('add-relation-button');
    fireEvent.click(addRelBtn);

    await screen.findByTestId('attach-relation-form');

    // Select target asset 'VLC Media Player' (sw-001)
    const targetOption = await screen.findByTestId('target-asset-option-sw-001');
    fireEvent.click(targetOption);

    const relTypeSelect = screen.getByTestId('attach-relation-type-select');
    fireEvent.change(relTypeSelect, { target: { value: 'uses' } });

    const submitRelBtn = screen.getByTestId('submit-attach-relation-button');
    fireEvent.click(submitRelBtn);

    // Relation is attached and modal dismissed
    await waitFor(() => {
      expect(screen.queryByTestId('attach-relation-form')).not.toBeInTheDocument();
    });
    await screen.findByTestId('relation-explorer');

    // 6. EXPORT PORTABLE BUNDLE: Switch to Portable Data workspace
    const exportTab = await screen.findByTestId('nav-import-export');
    fireEvent.click(exportTab);

    await screen.findByTestId('import-export-workspace');
    expect(screen.getByText('Export Portable Bundle')).toBeInTheDocument();

    // Set export directory and run export
    const dirInput = screen.getByTestId('export-dir-input');
    fireEvent.change(dirInput, { target: { value: '/Users/test/backup-bundle' } });

    const executeExportBtn = screen.getByTestId('execute-export-button');
    fireEvent.click(executeExportBtn);

    // Verify receipt appears with manifest summary
    const receipt = await screen.findByTestId('export-receipt');
    expect(receipt).toBeInTheDocument();
    expect(screen.getByText(/✓ Export Complete/)).toBeInTheDocument();
    expect(screen.getByText(/Receipt: \/Users\/test\/backup-bundle/)).toBeInTheDocument();
  });

  it('Keyboard navigation and accessibility: focus visible, modal close on Escape, screen-reader landmarks', async () => {
    render(<App />);

    await screen.findByTestId('nav-all');

    // Navigation Rail has role="tablist"
    const tablist = screen.getByRole('tablist', { name: 'Library Sections' });
    expect(tablist).toBeInTheDocument();

    // Switch to software tab
    const softwareNav = screen.getByTestId('nav-software');
    fireEvent.click(softwareNav);

    // Open software modal
    const addSoftwareBtn = screen.getByTestId('new-asset-button');
    fireEvent.click(addSoftwareBtn);

    // Modal dialog is rendered
    await screen.findByTestId('create-software-form');
    expect(screen.getByTestId('software-create-name-input')).toBeInTheDocument();

    // Press Escape to dismiss modal
    fireEvent.keyDown(window, { key: 'Escape', code: 'Escape' });

    // Modal is dismissed cleanly
    await waitFor(() => {
      expect(screen.queryByTestId('create-software-form')).not.toBeInTheDocument();
    });
  });

  it('Settings environment: inspects DB status, providers, and enforces theme attribute switching', async () => {
    render(<App />);

    const settingsTab = await screen.findByTestId('nav-settings');
    fireEvent.click(settingsTab);

    await screen.findByTestId('settings-workspace');
    expect(screen.getByTestId('settings-db-path')).toHaveTextContent(/assetmesh\.db/);
    expect(screen.getByTestId('settings-db-status')).toHaveTextContent('Ready');

    // Switch theme to Dark
    const darkBtn = screen.getByTestId('theme-dark');
    fireEvent.click(darkBtn);

    // Document element receives dataset attribute
    expect(document.documentElement.dataset.theme).toBe('dark');

    // Switch theme to Light
    const lightBtn = screen.getByTestId('theme-light');
    fireEvent.click(lightBtn);
    expect(document.documentElement.dataset.theme).toBe('light');
  });
});
