import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { App } from '../../app/App';
import { ActivityFeed } from './ActivityFeed';
import { DuplicateReview } from './DuplicateReview';
import { MergeModal } from './MergeModal';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary, DuplicateCandidateDto } from './types';

const softwareA: AssetSummary = {
  id: 'asset-sw-001',
  kind: 'software.cli',
  name: 'ripgrep',
  lifecycle: 'active',
  subtitle: 'Fast line-oriented search tool',
  tags: ['search', 'cli'],
  updated_at: '2024-03-20T10:00:00Z',
};

const softwareB: AssetSummary = {
  id: 'asset-sw-002',
  kind: 'software.cli',
  name: 'RipGrep',
  lifecycle: 'active',
  subtitle: 'Duplicate ripgrep build',
  tags: ['search', 'rust'],
  updated_at: '2024-03-21T10:00:00Z',
};

const serviceA: AssetSummary = {
  id: 'asset-srv-001',
  kind: 'service.saas',
  name: 'OpenAI Pro',
  lifecycle: 'active',
  subtitle: 'AI API service account',
  tags: ['ai'],
  updated_at: '2024-03-20T10:00:00Z',
};

const serviceB: AssetSummary = {
  id: 'asset-srv-002',
  kind: 'service.saas',
  name: 'OpenAI Team',
  lifecycle: 'active',
  subtitle: 'Another AI service account',
  tags: ['ai'],
  updated_at: '2024-03-21T10:00:00Z',
};

describe('Activity and Duplicate Review Workflow UI (P5-08)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([
      JSON.parse(JSON.stringify(softwareA)),
      JSON.parse(JSON.stringify(softwareB)),
      JSON.parse(JSON.stringify(serviceA)),
      JSON.parse(JSON.stringify(serviceB)),
    ]);
    setTransport(fakeTransport);
  });

  it('Flow 1 (Global Activity Feed & Filters): lists events, filters by module and kind, toggles payload', async () => {
    fakeTransport.recordActivity(
      'software.created',
      softwareA.id,
      softwareA.name,
      { version: '14.1.0' },
      'software'
    );
    fakeTransport.recordActivity(
      'service.created',
      serviceA.id,
      serviceA.name,
      { provider: 'OpenAI' },
      'services'
    );

    render(<ActivityFeed />);

    // Wait for events to load
    await screen.findByTestId('activity-feed-container');
    expect(screen.getByText('Global Activity Feed')).toBeInTheDocument();

    // Verify events present
    expect(screen.getAllByText('software.created').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('service.created').length).toBeGreaterThanOrEqual(1);

    // Toggle payload
    const payloadToggle = screen.getByTestId(`activity-payload-toggle-${fakeTransport.activityEvents[0].id}`);
    fireEvent.click(payloadToggle);
    expect(screen.getByTestId(`activity-payload-${fakeTransport.activityEvents[0].id}`)).toBeInTheDocument();

    // Filter by module 'software'
    const moduleSelect = screen.getByTestId('activity-module-filter');
    fireEvent.change(moduleSelect, { target: { value: 'software' } });

    await waitFor(() => {
      expect(screen.getAllByText('software.created').length).toBeGreaterThanOrEqual(1);
      expect(screen.queryByText('service.created')).not.toBeInTheDocument();
    });
  });

  it('Flow 1b (Activity kind filter): narrows below the module', async () => {
    // docs/12 requires activity filtering by kind, not only by module. Both
    // assets here are Software, so the module filter would accept both — only
    // the kind separates `software.cli` from `software.app`.
    fakeTransport.recordActivity('software.created', softwareA.id, softwareA.name, {}, 'software');
    fakeTransport.recordActivity('service.created', serviceA.id, serviceA.name, {}, 'services');

    // A third asset in the same module but a different kind, so the assertion
    // below proves the kind filter is narrower than the module filter.
    fakeTransport.assets.push({
      id: 'asset-sw-app-001',
      kind: 'software.app',
      name: 'VLC',
      lifecycle: 'active',
      subtitle: null,
      tags: [],
      updated_at: '2024-03-22T10:00:00Z',
    });
    fakeTransport.recordActivity('software.created', 'asset-sw-app-001', 'VLC', {}, 'software');

    render(
      <ActivityFeed
        capabilities={
          {
            asset_kinds: ['software.cli', 'software.app', 'service.saas'],
            modules: [],
            relation_types: [],
            storable_relation_types: [],
            features: {
              runtime_enrichment: false,
              projects: false,
              agent_capabilities: false,
              knowledge_collections: false,
            },
          } as never
        }
      />
    );
    await screen.findByTestId('activity-feed-container');

    // Both software events show before anything is filtered. `getAllByText`
    // because the fixture seeds an event for this asset already.
    expect(screen.getAllByText('ripgrep').length).toBeGreaterThan(0);
    expect(screen.getAllByText('VLC').length).toBeGreaterThan(0);

    // One kind keeps only its own asset's events.
    fireEvent.change(screen.getByTestId('activity-kind-filter'), {
      target: { value: 'software.cli' },
    });
    await waitFor(() => {
      expect(screen.queryByText('VLC')).not.toBeInTheDocument();
    });
    expect(screen.getAllByText('ripgrep').length).toBeGreaterThan(0);
    expect(screen.queryByText('OpenAI Pro')).not.toBeInTheDocument();

    // Switching kinds re-queries for the other one rather than accumulating.
    fireEvent.change(screen.getByTestId('activity-kind-filter'), {
      target: { value: 'software.app' },
    });
    await waitFor(() => {
      expect(screen.getAllByText('VLC').length).toBeGreaterThan(0);
    });
    expect(screen.queryByText('ripgrep')).not.toBeInTheDocument();

    // Reset clears the kind too, so the reset button disappears with it.
    fireEvent.click(screen.getByTestId('reset-activity-filters'));
    await waitFor(() => {
      expect(screen.queryByTestId('reset-activity-filters')).not.toBeInTheDocument();
    });
    expect(screen.getAllByText('ripgrep').length).toBeGreaterThan(0);
    expect(screen.getAllByText('VLC').length).toBeGreaterThan(0);
  });

  it('Flow 2 (Asset-Scoped Activity History): renders activity section within AssetDetailView', async () => {
    fakeTransport.recordActivity(
      'software.updated',
      softwareA.id,
      softwareA.name,
      { field: 'notes' },
      'software'
    );

    render(<AssetDetailView assetId={softwareA.id} />);

    await screen.findByTestId('asset-detail-view');

    // Activity section is present
    const activitySection = screen.getByTestId('asset-activity-section');
    expect(activitySection).toBeInTheDocument();

    // Click Show History
    const toggleBtn = screen.getByTestId('toggle-asset-activity-button');
    expect(toggleBtn).toHaveTextContent('Show History');
    fireEvent.click(toggleBtn);

    // Activity feed appears scoped to softwareA
    await screen.findByText('software.updated');
    expect(screen.getByText('Asset Activity History')).toBeInTheDocument();
  });

  it('Flow 3 (Duplicate Candidate Review): displays candidates with concrete evidence and supports dismissal', async () => {
    render(<DuplicateReview capabilities={fakeTransport.capabilities} />);

    await screen.findByTestId('duplicate-review-container');

    // Should find the candidate pair for ripgrep and RipGrep
    const pairKey = `${softwareA.id}:${softwareB.id}`;
    await screen.findByTestId(`candidate-card-${pairKey}`);

    // Evidence badge is present
    expect(screen.getByTestId('candidate-evidence-badge')).toHaveTextContent(
      'same normalized name (ripgrep)'
    );

    // Ephemeral dismiss
    const dismissBtn = screen.getByTestId(`dismiss-candidate-${pairKey}`);
    fireEvent.click(dismissBtn);

    // Candidate should disappear from visible view
    await waitFor(() => {
      expect(screen.queryByTestId(`candidate-card-${pairKey}`)).not.toBeInTheDocument();
    });
    expect(screen.getByText('No duplicate candidates detected.')).toBeInTheDocument();

    // Reset dismissed restores it
    const resetBtn = screen.getByTestId('reset-dismissed-button');
    fireEvent.click(resetBtn);

    await screen.findByTestId(`candidate-card-${pairKey}`);
  });

  it('Flow 4 (Explicit Winner Selection & Merge Preview Modal): switches winner and shows preview', async () => {
    const candidate: DuplicateCandidateDto = {
      left: softwareA,
      right: softwareB,
      evidence: [{ evidence: 'same_normalized_name', normalized_name: 'ripgrep', kind: 'software.cli' }],
      evidence_labels: ['same normalized name (ripgrep)'],
    };

    const previewSpy = vi.spyOn(fakeTransport, 'mergePreview');

    render(
      <MergeModal
        candidate={candidate}
        isOpen={true}
        onClose={vi.fn()}
        onMerged={vi.fn()}
      />
    );

    await screen.findByTestId('merge-modal');
    expect(screen.getByText('Explicit Merge Review')).toBeInTheDocument();

    // No winner is preselected and no preview is fetched on open: the
    // completion condition for P5-08 is that the UI must not choose a winner by
    // similarity. Choosing is the user's step, and the preview follows it.
    expect(previewSpy).not.toHaveBeenCalled();
    expect(screen.getByTestId('merge-choice-required')).toBeInTheDocument();
    expect(screen.getByTestId('radio-select-left')).not.toBeChecked();
    expect(screen.getByTestId('radio-select-right')).not.toBeChecked();
    expect(screen.getByTestId('execute-merge-button')).toBeDisabled();

    // Choosing the left candidate is what starts the review.
    fireEvent.click(screen.getByTestId('radio-select-left'));
    await screen.findByTestId('merge-preview-container');
    expect(previewSpy).toHaveBeenCalledWith(softwareA.id, softwareB.id);
    expect(screen.getByTestId('merge-ready-alert')).toBeInTheDocument();

    // Switching the winner to the right re-runs the preview the other way.
    fireEvent.click(screen.getByTestId('radio-select-right'));

    await waitFor(() => {
      expect(previewSpy).toHaveBeenCalledWith(softwareB.id, softwareA.id);
    });
  });

  it('Flow 5 (Merge Apply & Redirection): executes merge, updates survivor and tombstones loser', async () => {
    const candidate: DuplicateCandidateDto = {
      left: softwareA,
      right: softwareB,
      evidence: [{ evidence: 'same_normalized_name', normalized_name: 'ripgrep', kind: 'software.cli' }],
      evidence_labels: ['same normalized name (ripgrep)'],
    };

    const mergeSpy = vi.spyOn(fakeTransport, 'mergeApply');
    const onMerged = vi.fn();

    render(
      <MergeModal
        candidate={candidate}
        isOpen={true}
        onClose={vi.fn()}
        onMerged={onMerged}
      />
    );

    // Choose the survivor explicitly before anything else can happen.
    fireEvent.click(screen.getByTestId('radio-select-left'));
    await screen.findByTestId('merge-preview-container');

    // Check confirmation checkbox
    const confirmBox = screen.getByTestId('merge-confirm-checkbox');
    fireEvent.click(confirmBox);

    // Execute merge
    const executeBtn = screen.getByTestId('execute-merge-button');
    expect(executeBtn).not.toBeDisabled();
    fireEvent.click(executeBtn);

    await waitFor(() => {
      expect(mergeSpy).toHaveBeenCalledWith({
        winner_id: softwareA.id,
        loser_id: softwareB.id,
        expected_winner_revision: 1,
        expected_loser_revision: 1,
      });
      expect(onMerged).toHaveBeenCalled();
    });

    // Verify loser is now merged tombstone in fakeTransport
    const loserSummary = fakeTransport.assets.find((a) => a.id === softwareB.id);
    expect(loserSummary?.lifecycle).toBe('merged');
  });

  it('Flow 6 (Conflict Detection & Blocked Merge): prevents merge when field conflicts exist', async () => {
    // Setup service detail records with conflicting plans
    fakeTransport.details.set(serviceA.id, {
      id: serviceA.id,
      kind: serviceA.kind,
      name: serviceA.name,
      summary: serviceA.subtitle,
      lifecycle: 'active',
      revision: 1,
      created_at: serviceA.updated_at,
      updated_at: serviceA.updated_at,
      archived_at: null,
      merged_into: null,
      tags: serviceA.tags,
      external_refs: [],
      details: {
        module: 'services',
        asset_id: serviceA.id,
        service_type: 'saas',
        provider: 'OpenAI',
        account_label: 'Personal',
        endpoint_url: null,
        dashboard_url: null,
        domain_name: null,
        plan: 'Plus',
        cost_minor: 2000,
        currency: 'USD',
        billing_cadence: 'monthly',
        renews_at: null,
        expires_at: null,
        auto_renew: true,
        notes: null,
      },
    });

    fakeTransport.details.set(serviceB.id, {
      id: serviceB.id,
      kind: serviceB.kind,
      name: serviceB.name,
      summary: serviceB.subtitle,
      lifecycle: 'active',
      revision: 1,
      created_at: serviceB.updated_at,
      updated_at: serviceB.updated_at,
      archived_at: null,
      merged_into: null,
      tags: serviceB.tags,
      external_refs: [],
      details: {
        module: 'services',
        asset_id: serviceB.id,
        service_type: 'saas',
        provider: 'OpenAI',
        account_label: 'Personal',
        endpoint_url: null,
        dashboard_url: null,
        domain_name: null,
        plan: 'Team',
        cost_minor: 3000,
        currency: 'USD',
        billing_cadence: 'monthly',
        renews_at: null,
        expires_at: null,
        auto_renew: true,
        notes: null,
      },
    });

    const candidate: DuplicateCandidateDto = {
      left: serviceA,
      right: serviceB,
      evidence: [{ evidence: 'same_provider', provider: 'OpenAI' }],
      evidence_labels: ['same provider (OpenAI)'],
    };

    render(
      <MergeModal
        candidate={candidate}
        isOpen={true}
        onClose={vi.fn()}
        onMerged={vi.fn()}
      />
    );

    // The conflict is about a specific pair, so the survivor has to be chosen
    // before it can be reported.
    expect(screen.getByTestId('merge-choice-required')).toBeInTheDocument();
    fireEvent.click(screen.getByTestId('radio-select-left'));

    // Conflict alert must appear
    await screen.findByTestId('merge-conflict-alert');
    expect(screen.getByText(/Merge Blocked/)).toBeInTheDocument();

    // Execute Merge button is disabled
    const executeBtn = screen.getByTestId('execute-merge-button');
    expect(executeBtn).toBeDisabled();

    // Switching to the other survivor does not unblock it: the conflict is
    // between the two records, whichever way round they are merged.
    fireEvent.click(screen.getByTestId('radio-select-right'));
    await waitFor(() => {
      expect(screen.getByTestId('merge-conflict-alert')).toBeInTheDocument();
    });
    expect(screen.getByTestId('execute-merge-button')).toBeDisabled();
  });

  it('Flow 7 (Navigation Rail - Workspaces): navigates to Activity and Duplicates sections', async () => {
    render(<App />);

    // Click Activity tab in navigation rail
    const actTab = await screen.findByTestId('nav-activity');
    fireEvent.click(actTab);

    await screen.findByTestId('activity-workspace');
    expect(screen.getByText('Global Activity Feed')).toBeInTheDocument();

    // Click Duplicates tab in navigation rail
    const dupTab = screen.getByTestId('nav-duplicates');
    fireEvent.click(dupTab);

    await screen.findByTestId('duplicates-workspace');
    expect(screen.getByText('Duplicate Review')).toBeInTheDocument();
  });

  it('Flow 8 (Optimistic Concurrency & Stale Revision Handling): preserves user selection and displays merge-error-banner upon concurrent modification', async () => {
    const candidate: DuplicateCandidateDto = {
      left: softwareA,
      right: softwareB,
      evidence: [{ evidence: 'same_normalized_name', normalized_name: 'ripgrep', kind: 'software.cli' }],
      evidence_labels: ['same normalized name (ripgrep)'],
    };

    // Mock mergeApply to throw a stale_revision error
    const staleErr = new Error('Asset revision mismatch: expected 1, found 2') as Error & { category?: string };
    staleErr.category = 'stale_revision';
    vi.spyOn(fakeTransport, 'mergeApply').mockRejectedValueOnce(staleErr);

    render(
      <MergeModal
        candidate={candidate}
        isOpen={true}
        onClose={vi.fn()}
        onMerged={vi.fn()}
      />
    );

    // Select left candidate
    fireEvent.click(screen.getByTestId('radio-select-left'));
    await screen.findByTestId('merge-preview-container');

    // Confirm and execute merge
    const confirmBox = await screen.findByTestId('merge-confirm-checkbox');
    fireEvent.click(confirmBox);
    const executeBtn = screen.getByTestId('execute-merge-button');
    expect(executeBtn).not.toBeDisabled();
    fireEvent.click(executeBtn);

    // Banner appears with stale_revision error
    const banner = await screen.findByTestId('merge-error-banner');
    expect(banner).toBeInTheDocument();
    expect(banner).toHaveTextContent('stale_revision');

    // Selection is preserved! User does NOT need to reselect winner
    expect(screen.getByTestId('radio-select-left')).toBeChecked();
    expect(screen.getByTestId('merge-preview-container')).toBeInTheDocument();
  });
});
