import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { App } from '../../app/App';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary } from './types';

const initialSoftwareAsset: AssetSummary = {
  id: 'asset-software-001',
  kind: 'software.tool',
  name: 'Sublime Text',
  lifecycle: 'active',
  subtitle: 'Fast and responsive text editor',
  tags: ['editor', 'gui'],
  updated_at: '2024-03-20T10:00:00Z',
};

describe('Software Workflow UI (P5-06)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([initialSoftwareAsset]);
    setTransport(fakeTransport);
  });

  it('Flow 1 (Add Software): opens creation modal, submits valid software input, updates ledger', async () => {
    const softwareCmdSpy = vi.spyOn(fakeTransport, 'softwareCommand');

    render(<App />);

    // Switch to Software module via rail
    const softwareNavBtn = await screen.findByTestId('nav-software');
    fireEvent.click(softwareNavBtn);

    // Wait for ledger to load
    await screen.findAllByText('Sublime Text');

    // Click "+ Add Software" button in header
    const addBtn = screen.getByTestId('new-asset-button');
    expect(addBtn).toHaveTextContent('+ Add Software');
    fireEvent.click(addBtn);

    // Modal appears
    await screen.findByTestId('create-software-form');

    // Fill form
    fireEvent.change(screen.getByTestId('software-create-name-input'), {
      target: { value: 'Neovim' },
    });
    fireEvent.change(screen.getByTestId('software-create-category-select'), {
      target: { value: 'tool' },
    });
    fireEvent.change(screen.getByTestId('software-create-source-select'), {
      target: { value: 'homebrew_formula' },
    });
    fireEvent.change(screen.getByTestId('software-create-version-input'), {
      target: { value: '0.9.5' },
    });
    fireEvent.change(screen.getByTestId('software-create-summary-input'), {
      target: { value: 'Vim-fork focused on extensibility' },
    });
    fireEvent.change(screen.getByTestId('software-create-purpose-input'), {
      target: { value: 'Terminal text editor' },
    });
    fireEvent.change(screen.getByTestId('software-create-notes-input'), {
      target: { value: 'Configured with lua' },
    });
    fireEvent.change(screen.getByTestId('software-create-tags-input'), {
      target: { value: 'cli, editor, vim' },
    });

    // Submit
    fireEvent.submit(screen.getByTestId('create-software-form'));

    await waitFor(() => {
      expect(softwareCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'create',
          name: 'Neovim',
          category: 'tool',
          install_source: 'homebrew_formula',
          version: '0.9.5',
          purpose: 'Terminal text editor',
          notes: 'Configured with lua',
          tags: ['cli', 'editor', 'vim'],
        })
      );
    });

    // Modal closes and new asset is listed
    await waitFor(() => {
      expect(screen.queryByTestId('create-software-form')).not.toBeInTheDocument();
      expect(screen.getAllByText('Neovim').length).toBeGreaterThan(0);
    });
  });

  it('Flow 2 (Discovery and Adoption): scans system, displays candidates, adopts candidate preserving user fields', async () => {
    const discoverSpy = vi.spyOn(fakeTransport, 'softwareDiscover');
    const softwareCmdSpy = vi.spyOn(fakeTransport, 'softwareCommand');

    render(<App />);

    // Switch to Software module
    const softwareNavBtn = await screen.findByTestId('nav-software');
    fireEvent.click(softwareNavBtn);

    // Wait for ledger
    await screen.findAllByText('Sublime Text');

    // Click "Scan System" button
    const scanBtn = screen.getByTestId('discover-software-button');
    fireEvent.click(scanBtn);

    await waitFor(() => {
      expect(discoverSpy).toHaveBeenCalled();
    });

    // Discovery modal appears and lists candidates
    const candidateItem = await screen.findByTestId('discovery-candidate-ripgrep');
    expect(candidateItem).toBeInTheDocument();
    fireEvent.click(candidateItem);

    // Fill in adoption form
    const purposeInput = screen.getByTestId('adopt-purpose-input');
    const notesInput = screen.getByTestId('adopt-notes-input');
    const tagsInput = screen.getByTestId('adopt-tags-input');

    fireEvent.change(purposeInput, { target: { value: 'Fast search tool for codebases' } });
    fireEvent.change(notesInput, { target: { value: 'Adopted from homebrew' } });
    fireEvent.change(tagsInput, { target: { value: 'cli, search' } });

    // Submit adoption
    fireEvent.submit(screen.getByTestId('adopt-candidate-form'));

    await waitFor(() => {
      expect(softwareCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'adopt_candidate',
          purpose: 'Fast search tool for codebases',
          notes: 'Adopted from homebrew',
          tags: ['cli', 'search'],
          candidate: expect.objectContaining({
            display_name: 'ripgrep',
            install_source: 'homebrew_formula',
          }),
        })
      );
    });

    // Success receipt displayed
    await screen.findByTestId('adopt-success-receipt');
  });

  it('Flow 3 (Edit Metadata): edits purpose, notes, version with read-back', async () => {
    const onUpdated = vi.fn();
    const softwareCmdSpy = vi.spyOn(fakeTransport, 'softwareCommand');

    render(
      <AssetDetailView
        assetId="asset-software-001"
        onAssetUpdated={onUpdated}
      />
    );

    await screen.findByTestId('asset-detail-view');

    // Click edit software button
    const editBtn = screen.getByTestId('edit-software-button');
    fireEvent.click(editBtn);

    const form = screen.getByTestId('software-edit-form');
    expect(form).toBeInTheDocument();

    const purposeInput = screen.getByTestId('software-purpose-input') as HTMLInputElement;
    const notesInput = screen.getByTestId('software-notes-input') as HTMLTextAreaElement;

    fireEvent.change(purposeInput, { target: { value: 'Primary text editor' } });
    fireEvent.change(notesInput, { target: { value: 'License renewed for version 4' } });

    // Submit
    fireEvent.click(screen.getByTestId('save-software-button'));

    await waitFor(() => {
      expect(softwareCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'update_metadata',
          asset_id: 'asset-software-001',
          expected_revision: 1,
          purpose: 'Primary text editor',
          notes: 'License renewed for version 4',
        })
      );
    });

    // Read back closed form and updated presentation
    await waitFor(() => {
      expect(screen.queryByTestId('software-edit-form')).not.toBeInTheDocument();
      expect(screen.getByText('Primary text editor')).toBeInTheDocument();
      expect(screen.getByText('License renewed for version 4')).toBeInTheDocument();
      expect(screen.getByText('rev 2')).toBeInTheDocument();
    });

    expect(onUpdated).toHaveBeenCalled();
  });

  it('Flow 4 (Archive): prompts confirmation, executes software archive, makes read-only', async () => {
    const onUpdated = vi.fn();
    const softwareCmdSpy = vi.spyOn(fakeTransport, 'softwareCommand');

    render(
      <AssetDetailView
        assetId="asset-software-001"
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
      expect(softwareCmdSpy).toHaveBeenCalledWith({
        action: 'archive',
        asset_id: 'asset-software-001',
        expected_revision: 1,
      });
    });

    // Read back shows archived banner and active action buttons disappear
    await waitFor(() => {
      expect(screen.getByText(/Archived Asset:/)).toBeInTheDocument();
      expect(screen.queryByTestId('archive-asset-button')).not.toBeInTheDocument();
      expect(screen.queryByTestId('edit-software-button')).not.toBeInTheDocument();
    });

    expect(onUpdated).toHaveBeenCalled();
  });
});
