import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeDesktopTransport } from '../../test/fake-transport';
import { AssetDetailView } from './AssetDetailView';
import { setTransport } from './transport';
import type { AssetDetailDto } from './types';

const sampleMediaDetail: AssetDetailDto = {
  id: '019315d0-7a00-7000-8000-000000000001',
  kind: 'media.anime',
  name: 'Sousou no Frieren',
  summary: 'Elven mage on journey',
  lifecycle: 'active',
  revision: 1,
  created_at: '2026-09-22T10:00:00Z',
  updated_at: '2026-09-22T10:00:00Z',
  archived_at: null,
  merged_into: null,
  details: {
    module: 'media',
    asset_id: '019315d0-7a00-7000-8000-000000000001',
    media_type: 'anime',
    status: 'completed',
    rating: 9.8,
    year: 2023,
    platform: 'Crunchyroll',
    progress: { unit: 'episodes', current: 28, total: 28 },
    notes: 'Outstanding fantasy storytelling',
    started_at: '2023-09-29T00:00:00Z',
    completed_at: '2024-03-22T00:00:00Z',
  },
  tags: ['fantasy', 'magic'],
  external_refs: [
    {
      namespace: 'myanimelist',
      external_id: '52991',
      source_url: 'https://myanimelist.net/anime/52991',
    },
  ],
};

const sampleSoftwareDetail: AssetDetailDto = {
  id: '019315d0-7a00-7000-8000-000000000002',
  kind: 'software.tool',
  name: 'Neovim',
  summary: 'Modal text editor',
  lifecycle: 'active',
  revision: 3,
  created_at: '2026-09-22T09:00:00Z',
  updated_at: '2026-09-22T09:00:00Z',
  archived_at: null,
  merged_into: null,
  details: {
    module: 'software',
    asset_id: '019315d0-7a00-7000-8000-000000000002',
    category: 'tool',
    version: '0.10.1',
    install_location: '/opt/homebrew/bin',
    executable_path: '/opt/homebrew/bin/nvim',
    purpose: 'Daily terminal text editing',
    notes: 'Configured with Lua plugins',
    discovered_at: '2024-01-01T00:00:00Z',
    installed_at: '2024-01-01T00:00:00Z',
    architecture: 'arm64',
  },
  tags: ['editor', 'cli'],
  external_refs: [
    {
      namespace: 'homebrew',
      external_id: 'neovim',
      source_url: 'https://formulae.brew.sh/formula/neovim',
    },
  ],
};

const sampleServiceDetail: AssetDetailDto = {
  id: '019315d0-7a00-7000-8000-000000000003',
  kind: 'service.saas',
  name: 'GitHub Copilot',
  summary: 'AI code assistant',
  lifecycle: 'active',
  revision: 2,
  created_at: '2026-09-22T08:00:00Z',
  updated_at: '2026-09-22T08:00:00Z',
  archived_at: null,
  merged_into: null,
  details: {
    module: 'services',
    asset_id: '019315d0-7a00-7000-8000-000000000003',
    service_type: 'saas',
    provider: 'GitHub Inc.',
    account_label: 'Personal Account',
    endpoint_url: null,
    dashboard_url: 'https://github.com/settings/copilot',
    domain_name: null,
    plan: 'Individual',
    cost_minor: 1000,
    currency: 'USD',
    billing_cadence: 'monthly',
    renews_at: '2026-10-01T00:00:00Z',
    expires_at: null,
    auto_renew: true,
    notes: 'Used in VS Code and Neovim',
  },
  tags: ['ai', 'developer'],
  external_refs: [],
};

describe('AssetDetailView', () => {
  it('renders media details panel with all typed fields', async () => {
    const fake = new FakeDesktopTransport();
    fake.details.set(sampleMediaDetail.id, sampleMediaDetail);
    setTransport(fake);

    render(<AssetDetailView assetId={sampleMediaDetail.id} />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Sousou no Frieren' })).toBeInTheDocument();
      expect(screen.getByTestId('media-panel')).toBeInTheDocument();
    });

    expect(screen.getByText('anime')).toBeInTheDocument();
    expect(screen.getByText('completed')).toBeInTheDocument();
    expect(screen.getByText(/★ 9.8 \/ 10/)).toBeInTheDocument();
    expect(screen.getByText('28 / 28')).toBeInTheDocument();
    expect(screen.getByText('Outstanding fantasy storytelling')).toBeInTheDocument();
    expect(screen.getByTestId('external-refs-panel')).toBeInTheDocument();
    expect(screen.getByText('myanimelist:')).toBeInTheDocument();
    expect(screen.getByText('52991')).toBeInTheDocument();
  });

  it('renders software details panel with all typed fields', async () => {
    const fake = new FakeDesktopTransport();
    fake.details.set(sampleSoftwareDetail.id, sampleSoftwareDetail);
    setTransport(fake);

    render(<AssetDetailView assetId={sampleSoftwareDetail.id} />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Neovim' })).toBeInTheDocument();
      expect(screen.getByTestId('software-panel')).toBeInTheDocument();
    });

    expect(screen.getByText('0.10.1')).toBeInTheDocument();
    expect(screen.getByText('arm64')).toBeInTheDocument();
    expect(screen.getByText('Daily terminal text editing')).toBeInTheDocument();
    expect(screen.getByText('/opt/homebrew/bin/nvim')).toBeInTheDocument();
  });

  it('renders service details panel with currency, cadence, and dashboard link', async () => {
    const fake = new FakeDesktopTransport();
    fake.details.set(sampleServiceDetail.id, sampleServiceDetail);
    setTransport(fake);

    render(<AssetDetailView assetId={sampleServiceDetail.id} />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'GitHub Copilot' })).toBeInTheDocument();
      expect(screen.getByTestId('service-panel')).toBeInTheDocument();
    });

    expect(screen.getByText('GitHub Inc.')).toBeInTheDocument();
    expect(screen.getByText('Individual')).toBeInTheDocument();
    expect(screen.getByText('10.00 USD / monthly')).toBeInTheDocument();
    expect(screen.getByText('Auto-Renew Active')).toBeInTheDocument();
    expect(screen.getByText(/https:\/\/github.com\/settings\/copilot/)).toBeInTheDocument();
  });

  it('renders archived asset with prominent read-only banner', async () => {
    const archivedDetail: AssetDetailDto = {
      ...sampleMediaDetail,
      id: '019315d0-7a00-7000-8000-000000000004',
      lifecycle: 'archived',
      archived_at: '2026-09-01T00:00:00Z',
    };

    const fake = new FakeDesktopTransport();
    fake.details.set(archivedDetail.id, archivedDetail);
    setTransport(fake);

    render(<AssetDetailView assetId={archivedDetail.id} />);

    await waitFor(() => {
      expect(screen.getByText('Archived Asset:')).toBeInTheDocument();
      expect(screen.getByText(/This asset is preserved in read-only state/)).toBeInTheDocument();
      expect(screen.getByText(/Archived on 2026-09-01/)).toBeInTheDocument();
    });
  });

  it('renders merged redirect tombstone with navigation action to surviving asset', async () => {
    const survivorId = '019315d0-7a00-7000-8000-000000000003';
    const mergedDetail: AssetDetailDto = {
      id: '019315d0-7a00-7000-8000-000000000005',
      kind: 'service.saas',
      name: 'Old Copilot Entry',
      summary: null,
      lifecycle: 'merged',
      revision: 1,
      created_at: '2026-09-22T08:00:00Z',
      updated_at: '2026-09-22T08:00:00Z',
      archived_at: null,
      merged_into: survivorId,
      details: {
        module: 'merged_redirect',
        surviving_asset_id: survivorId,
      },
      tags: [],
      external_refs: [],
    };

    const onFollow = vi.fn();
    const fake = new FakeDesktopTransport();
    fake.details.set(mergedDetail.id, mergedDetail);
    setTransport(fake);

    render(
      <AssetDetailView
        assetId={mergedDetail.id}
        onFollowRedirect={onFollow}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('merged-redirect-panel')).toBeInTheDocument();
      expect(screen.getByText('This asset was explicitly merged')).toBeInTheDocument();
      expect(screen.getByText(survivorId)).toBeInTheDocument();
    });

    const redirectBtn = screen.getByRole('button', { name: 'View Survivor →' });
    await userEvent.click(redirectBtn);
    expect(onFollow).toHaveBeenCalledWith(survivorId);
  });

  it('renders safely when encountering unknown future module details', async () => {
    const futureDetail: AssetDetailDto = {
      id: '019315d0-7a00-7000-8000-000000000006',
      kind: 'quantum.computer',
      name: 'Future Asset',
      summary: 'From Phase 10',
      lifecycle: 'active',
      revision: 1,
      created_at: '2026-09-22T08:00:00Z',
      updated_at: '2026-09-22T08:00:00Z',
      archived_at: null,
      merged_into: null,
      details: {
        module: 'quantum_module_v99',
      } as unknown as AssetDetailDto['details'],
      tags: ['future'],
      external_refs: [],
    };

    const fake = new FakeDesktopTransport();
    fake.details.set(futureDetail.id, futureDetail);
    setTransport(fake);

    render(<AssetDetailView assetId={futureDetail.id} />);

    await waitFor(() => {
      expect(screen.getByTestId('unknown-panel')).toBeInTheDocument();
      expect(screen.getByText('Unknown Module Format')).toBeInTheDocument();
      expect(screen.getByText(/quantum_module_v99/)).toBeInTheDocument();
    });
  });

  it('handles missing optional fields cleanly without errors', async () => {
    const minimalDetail: AssetDetailDto = {
      id: '019315d0-7a00-7000-8000-000000000007',
      kind: 'media.movie',
      name: 'Minimal Movie',
      summary: null,
      lifecycle: 'active',
      revision: 1,
      created_at: '2026-09-22T08:00:00Z',
      updated_at: '2026-09-22T08:00:00Z',
      archived_at: null,
      merged_into: null,
      details: {
        module: 'media',
        asset_id: '019315d0-7a00-7000-8000-000000000007',
        media_type: 'movie',
        status: 'planned',
        rating: null,
        year: null,
        platform: null,
        progress: null,
        notes: null,
        started_at: null,
        completed_at: null,
      },
      tags: [],
      external_refs: [],
    };

    const fake = new FakeDesktopTransport();
    fake.details.set(minimalDetail.id, minimalDetail);
    setTransport(fake);

    render(<AssetDetailView assetId={minimalDetail.id} />);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Minimal Movie' })).toBeInTheDocument();
      expect(screen.getByTestId('media-panel')).toBeInTheDocument();
      expect(screen.getByText('planned')).toBeInTheDocument();
    });
  });
});
