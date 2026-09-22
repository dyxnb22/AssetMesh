import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { App } from '../../app/App';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary } from './types';

const initialMediaAsset: AssetSummary = {
  id: 'asset-media-001',
  kind: 'media.anime',
  name: 'Steins;Gate',
  lifecycle: 'active',
  subtitle: 'Time travel sci-fi masterpiece',
  tags: ['anime', 'sci-fi', 'thriller'],
  updated_at: '2024-03-20T10:00:00Z',
};

describe('Media Workflow UI (P5-06)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([initialMediaAsset]);
    setTransport(fakeTransport);
  });

  it('Flow 1 (Add Media): opens creation modal, submits valid media input, updates ledger and inspector', async () => {
    const mediaCmdSpy = vi.spyOn(fakeTransport, 'mediaCommand');

    render(<App />);

    // Wait for ledger to load
    await screen.findAllByText('Steins;Gate');

    // Click "+ Add Media" button in header
    const addBtn = screen.getByTestId('new-asset-button');
    fireEvent.click(addBtn);

    // Modal appears
    await screen.findByTestId('create-media-form');

    // Fill form
    fireEvent.change(screen.getByTestId('media-create-title-input'), {
      target: { value: 'Cowboy Bebop' },
    });
    fireEvent.change(screen.getByTestId('media-create-type-select'), {
      target: { value: 'anime' },
    });
    fireEvent.change(screen.getByTestId('media-create-status-select'), {
      target: { value: 'completed' },
    });
    fireEvent.change(screen.getByTestId('media-create-summary-input'), {
      target: { value: 'Bounty hunters in space' },
    });
    fireEvent.change(screen.getByTestId('media-create-rating-input'), {
      target: { value: '9.8' },
    });
    fireEvent.change(screen.getByTestId('media-create-year-input'), {
      target: { value: '1998' },
    });
    fireEvent.change(screen.getByTestId('media-create-platform-input'), {
      target: { value: 'Blu-ray' },
    });
    fireEvent.change(screen.getByTestId('media-create-unit-input'), {
      target: { value: 'sessions' },
    });
    fireEvent.change(screen.getByTestId('media-create-current-input'), {
      target: { value: '26' },
    });
    fireEvent.change(screen.getByTestId('media-create-total-input'), {
      target: { value: '26' },
    });
    fireEvent.change(screen.getByTestId('media-create-tags-input'), {
      target: { value: 'anime, space, jazz' },
    });

    // Submit
    fireEvent.submit(screen.getByTestId('create-media-form'));

    await waitFor(() => {
      expect(mediaCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'create',
          title: 'Cowboy Bebop',
          media_type: 'anime',
          status: 'completed',
          rating: 9.8,
          year: 1998,
          platform: 'Blu-ray',
          progress_unit: 'sessions',
          progress_current: 26,
          progress_total: 26,
          tags: ['anime', 'space', 'jazz'],
        })
      );
    });

    // Modal closes and new asset is listed in the ledger
    await waitFor(() => {
      expect(screen.queryByTestId('create-media-form')).not.toBeInTheDocument();
      expect(screen.getAllByText('Cowboy Bebop').length).toBeGreaterThan(0);
    });
  });

  it('Flow 2 (Edit Metadata): edits title, summary, year, platform, notes with read-back', async () => {
    const onUpdated = vi.fn();
    const mediaCmdSpy = vi.spyOn(fakeTransport, 'mediaCommand');

    render(
      <AssetDetailView
        assetId="asset-media-001"
        onAssetUpdated={onUpdated}
      />
    );

    await screen.findByTestId('asset-detail-view');

    // Click edit metadata button
    const editBtn = screen.getByTestId('edit-media-metadata-button');
    fireEvent.click(editBtn);

    const form = screen.getByTestId('media-edit-form');
    expect(form).toBeInTheDocument();

    const titleInput = screen.getByTestId('media-title-input') as HTMLInputElement;
    const summaryInput = screen.getByTestId('media-summary-input') as HTMLInputElement;
    const yearInput = screen.getByTestId('media-year-input') as HTMLInputElement;

    expect(titleInput.value).toBe('Steins;Gate');

    fireEvent.change(titleInput, { target: { value: 'Steins;Gate (Elite)' } });
    fireEvent.change(summaryInput, { target: { value: 'Remastered visual VN & anime' } });
    fireEvent.change(yearInput, { target: { value: '2018' } });

    // Submit
    fireEvent.click(screen.getByTestId('save-media-button'));

    await waitFor(() => {
      expect(mediaCmdSpy).toHaveBeenCalledWith(
        expect.objectContaining({
          action: 'update_metadata',
          asset_id: 'asset-media-001',
          expected_revision: 1,
          title: 'Steins;Gate (Elite)',
          summary: 'Remastered visual VN & anime',
          year: 2018,
        })
      );
    });

    // Verify read-back closed form and updated presentation
    await waitFor(() => {
      expect(screen.queryByTestId('media-edit-form')).not.toBeInTheDocument();
    });
    expect(screen.getByRole('heading', { level: 2, name: 'Steins;Gate (Elite)' })).toBeInTheDocument();
    expect(screen.getByText('Remastered visual VN & anime')).toBeInTheDocument();
    expect(screen.getByText('2018')).toBeInTheDocument();
    expect(screen.getByText('rev 2')).toBeInTheDocument();

    expect(onUpdated).toHaveBeenCalled();
  });

  it('Flow 3 (Status Transition): transitions status and updates read-back view', async () => {
    const onUpdated = vi.fn();
    const mediaCmdSpy = vi.spyOn(fakeTransport, 'mediaCommand');

    render(
      <AssetDetailView
        assetId="asset-media-001"
        onAssetUpdated={onUpdated}
      />
    );

    await screen.findByTestId('asset-detail-view');

    // Currently "completed", can reopen
    const reopenBtn = screen.getByTestId('media-action-reopen');
    fireEvent.click(reopenBtn);

    await waitFor(() => {
      expect(mediaCmdSpy).toHaveBeenCalledWith({
        action: 'transition_status',
        asset_id: 'asset-media-001',
        status: 'in_progress',
        expected_revision: 1,
      });
    });

    // Read back shows in_progress status
    await waitFor(() => {
      expect(screen.getByTestId('media-status-text')).toHaveTextContent('in_progress');
    });

    // From in_progress, can pause
    const pauseBtn = await screen.findByTestId('media-action-pause');
    fireEvent.click(pauseBtn);

    await waitFor(() => {
      expect(mediaCmdSpy).toHaveBeenCalledWith({
        action: 'transition_status',
        asset_id: 'asset-media-001',
        status: 'paused',
        expected_revision: 2,
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('media-status-text')).toHaveTextContent('paused');
    });
  });

  it('Flow 4 (Progress and Rating): updates progress and rating with read-back', async () => {
    const mediaCmdSpy = vi.spyOn(fakeTransport, 'mediaCommand');

    render(<AssetDetailView assetId="asset-media-001" />);
    await screen.findByTestId('asset-detail-view');

    // 1. Progress
    const editProgBtn = screen.getByTestId('edit-progress-button');
    fireEvent.click(editProgBtn);

    const currentInput = screen.getByTestId('media-progress-current-input');
    const totalInput = screen.getByTestId('media-progress-total-input');
    const unitInput = screen.getByTestId('media-progress-unit-input');

    fireEvent.change(currentInput, { target: { value: '12' } });
    fireEvent.change(totalInput, { target: { value: '24' } });
    fireEvent.change(unitInput, { target: { value: 'episodes' } });

    fireEvent.click(screen.getByTestId('save-progress-button'));

    await waitFor(() => {
      expect(mediaCmdSpy).toHaveBeenCalledWith({
        action: 'update_progress',
        asset_id: 'asset-media-001',
        current: 12,
        total: 24,
        unit: 'episodes',
        expected_revision: 1,
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('media-progress-text')).toHaveTextContent('12 / 24 episodes');
    });

    // 2. Rating
    const editRatingBtn = screen.getByTestId('edit-rating-button');
    fireEvent.click(editRatingBtn);

    const ratingInput = screen.getByTestId('media-rating-input');
    fireEvent.change(ratingInput, { target: { value: '10' } });

    fireEvent.click(screen.getByTestId('save-rating-button'));

    await waitFor(() => {
      expect(mediaCmdSpy).toHaveBeenCalledWith({
        action: 'rate',
        asset_id: 'asset-media-001',
        rating: 10,
        expected_revision: 2,
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId('media-rating-text')).toHaveTextContent('★ 10 / 10');
    });
  });

  it('Flow 5 (Archive): prompts confirmation, executes archive, sets asset read-only', async () => {
    const onUpdated = vi.fn();
    const mediaCmdSpy = vi.spyOn(fakeTransport, 'mediaCommand');

    render(
      <AssetDetailView
        assetId="asset-media-001"
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
      expect(mediaCmdSpy).toHaveBeenCalledWith({
        action: 'archive',
        asset_id: 'asset-media-001',
        expected_revision: 1,
      });
    });

    // Read back shows archived banner and active action buttons disappear
    await waitFor(() => {
      expect(screen.getByText(/Archived Asset:/)).toBeInTheDocument();
      expect(screen.queryByTestId('archive-asset-button')).not.toBeInTheDocument();
      expect(screen.queryByTestId('edit-media-metadata-button')).not.toBeInTheDocument();
    });

    expect(onUpdated).toHaveBeenCalled();
  });
});
