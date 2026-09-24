import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import { setTransport } from '../features/library/transport';
import type { AssetSummary } from '../features/library/types';
import { FakeDesktopTransport } from '../test/fake-transport';
import { App } from './App';

const sampleAssets: AssetSummary[] = [
  {
    id: '019315d0-7a00-7000-8000-000000000001',
    kind: 'media.anime',
    name: 'Sousou no Frieren',
    lifecycle: 'active',
    subtitle: 'TV Series · 28 episodes',
    tags: ['healing', 'fantasy'],
    updated_at: '2026-09-22T10:00:00Z',
  },
  {
    id: '019315d0-7a00-7000-8000-000000000002',
    kind: 'software.tool',
    name: 'Neovim',
    lifecycle: 'active',
    subtitle: 'Modal text editor',
    tags: ['editor', 'cli'],
    updated_at: '2026-09-22T09:00:00Z',
  },
  {
    id: '019315d0-7a00-7000-8000-000000000003',
    kind: 'service.saas',
    name: 'GitHub Copilot',
    lifecycle: 'active',
    subtitle: 'AI code completion tool',
    tags: ['ai', 'developer'],
    updated_at: '2026-09-22T08:00:00Z',
  },
  {
    id: '019315d0-7a00-7000-8000-000000000004',
    kind: 'media.movie',
    name: 'Dune: Part Two',
    lifecycle: 'archived',
    subtitle: 'Paul Atreides unites with Chani',
    tags: ['sci-fi'],
    updated_at: '2026-09-22T07:00:00Z',
  },
];

describe('App Desktop Shell', () => {
  beforeEach(() => {
    window.location.hash = '';
  });

  it('renders loading state initially', async () => {
    const fake = new FakeDesktopTransport();
    fake.status = { status: 'loading' };
    setTransport(fake);

    render(<App />);
    await waitFor(() => {
      expect(screen.getByRole('status')).toBeInTheDocument();
      expect(screen.getByText(/Opening library and verifying state/i)).toBeInTheDocument();
    });
  });

  it('renders setup failure state when database setup fails', async () => {
    const fake = new FakeDesktopTransport();
    fake.status = { status: 'setup_failure', message: 'unable to open database file' };
    setTransport(fake);

    render(<App />);
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
      expect(screen.getByText('Setup Required')).toBeInTheDocument();
      expect(screen.getByText(/unable to open database file/i)).toBeInTheDocument();
    });
  });

  it('re-initializes the database when Retry is pressed on the setup card', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    fake.status = {
      status: 'setup_failure',
      message: 'The database could not be opened. Check that its location exists and is writable.',
    };
    const inits: (string | undefined)[] = [];
    const originalInit = fake.init.bind(fake);
    fake.init = async (dbPath?: string) => {
      inits.push(dbPath);
      return originalInit(dbPath);
    };
    setTransport(fake);

    render(<App />);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument());
    expect(inits).toHaveLength(0);

    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() => expect(screen.queryByRole('button', { name: 'Retry' })).toBeNull());
    expect(inits).toEqual([undefined]);
  });

  it('opens a database at a picked folder when the location is changed', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    fake.status = { status: 'setup_failure', message: 'The database could not be opened.' };
    const inits: (string | undefined)[] = [];
    fake.init = async (dbPath?: string) => {
      inits.push(dbPath);
      return { status: 'ready', db_path: dbPath ?? 'default' };
    };
    fake.pickDirectory = async () => '/tmp/chosen';
    setTransport(fake);

    render(<App />);
    await waitFor(() =>
      expect(screen.getByRole('button', { name: /Choose another folder/ })).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole('button', { name: /Choose another folder/ }));

    await waitFor(() => expect(inits).toEqual(['/tmp/chosen/assetmesh.db']));
  });

  it('renders corrupt failure state when migrations/checksums fail', async () => {
    const fake = new FakeDesktopTransport();
    fake.status = { status: 'corrupt_failure', message: 'checksum mismatch in migration 0001' };
    setTransport(fake);

    render(<App />);
    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
      expect(screen.getByText('Corrupt Data')).toBeInTheDocument();
      expect(screen.getByText(/checksum mismatch/i)).toBeInTheDocument();
    });
  });

  it('renders cross-domain assets and updates inspector on click', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    setTransport(fake);

    render(<App />);

    // Default lifecycle is 'active', so 3 active assets are shown (archived excluded)
    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'All Assets' })).toBeInTheDocument();
      expect(screen.getByText('3 assets recorded')).toBeInTheDocument();
    });

    const rows = screen.getAllByRole('row');
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent('Sousou no Frieren');
    expect(rows[1]).toHaveTextContent('Neovim');
    expect(rows[2]).toHaveTextContent('GitHub Copilot');

    // Inspector shows details of the auto-selected first item
    expect(screen.getByRole('complementary', { name: 'Asset Inspector' })).toBeInTheDocument();
    expect(screen.getByText('019315d0-7a00-7000-8000-000000000001')).toBeInTheDocument();

    // Clicking second row updates inspector
    await userEvent.click(rows[1]);
    expect(screen.getByText('019315d0-7a00-7000-8000-000000000002')).toBeInTheDocument();
  });

  it('filters by lifecycle including archived assets', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('3 assets recorded')).toBeInTheDocument();
    });

    // Select Active + Archived
    const lifecycleSelect = screen.getByRole('combobox', { name: 'Filter by lifecycle' });
    fireEvent.change(lifecycleSelect, { target: { value: 'active_or_archived' } });

    await waitFor(() => {
      expect(screen.getByText('4 assets recorded')).toBeInTheDocument();
      expect(screen.getByText('Dune: Part Two')).toBeInTheDocument();
    });
  });

  it('filters by module via rail navigation', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('3 assets recorded')).toBeInTheDocument();
    });

    // Click Software in navigation rail
    const softwareTab = screen.getByRole('tab', { name: 'Software' });
    await userEvent.click(softwareTab);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Software Inventory' })).toBeInTheDocument();
      expect(screen.getByText('1 asset recorded')).toBeInTheDocument();
      expect(screen.getAllByText('Neovim').length).toBeGreaterThan(0);
      expect(screen.queryByText('Sousou no Frieren')).not.toBeInTheDocument();
    });
  });

  it('sorts assets deterministically by name ascending', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('3 assets recorded')).toBeInTheDocument();
    });

    const sortSelect = screen.getByRole('combobox', { name: 'Sort assets' });
    fireEvent.change(sortSelect, { target: { value: 'name_asc' } });

    await waitFor(() => {
      const rows = screen.getAllByRole('row');
      // Name A-Z: GitHub Copilot, Neovim, Sousou no Frieren
      expect(rows[0]).toHaveTextContent('GitHub Copilot');
      expect(rows[1]).toHaveTextContent('Neovim');
      expect(rows[2]).toHaveTextContent('Sousou no Frieren');
    });
  });

  it('navigates with keyboard arrows Up and Down', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('3 assets recorded')).toBeInTheDocument();
    });

    const ledger = screen.getByLabelText('Asset Ledger');

    // Down arrow moves selection to Neovim
    fireEvent.keyDown(ledger, { key: 'ArrowDown' });
    await waitFor(() => {
      expect(screen.getByText('019315d0-7a00-7000-8000-000000000002')).toBeInTheDocument();
    });

    // Down arrow moves selection to GitHub Copilot
    fireEvent.keyDown(ledger, { key: 'ArrowDown' });
    await waitFor(() => {
      expect(screen.getByText('019315d0-7a00-7000-8000-000000000003')).toBeInTheDocument();
    });

    // Up arrow moves selection back to Neovim
    fireEvent.keyDown(ledger, { key: 'ArrowUp' });
    await waitFor(() => {
      expect(screen.getByText('019315d0-7a00-7000-8000-000000000002')).toBeInTheDocument();
    });
  });

  it('handles pagination next and previous pages', async () => {
    // Generate 30 dummy assets to trigger pagination (pageSize = 25)
    const thirtyAssets: AssetSummary[] = Array.from({ length: 30 }, (_, i) => ({
      id: `019315d0-7a00-7000-8000-${String(i).padStart(12, '0')}`,
      kind: 'media.anime',
      name: `Anime Item #${String(i + 1).padStart(2, '0')}`,
      lifecycle: 'active',
      subtitle: `Description for #${i + 1}`,
      tags: ['test'],
      updated_at: `2026-09-22T${String(23 - (i % 24)).padStart(2, '0')}:00:00Z`,
    }));

    const fake = new FakeDesktopTransport(thirtyAssets);
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('Page 1 of 2 (30 total)')).toBeInTheDocument();
    });

    const prevBtn = screen.getByRole('button', { name: 'Previous Page' });
    const nextBtn = screen.getByRole('button', { name: 'Next Page' });

    expect(prevBtn).toBeDisabled();
    expect(nextBtn).toBeEnabled();

    // Click next page
    await userEvent.click(nextBtn);

    await waitFor(() => {
      expect(screen.getByText('Page 2 of 2 (30 total)')).toBeInTheDocument();
      expect(screen.getAllByRole('row')).toHaveLength(5);
    });

    expect(prevBtn).toBeEnabled();
    expect(nextBtn).toBeDisabled();
  });

  it('displays empty state with reset filters button', async () => {
    const fake = new FakeDesktopTransport([]);
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByText('No assets found')).toBeInTheDocument();
      expect(
        screen.getByText('No assets match the current view and filter criteria.')
      ).toBeInTheDocument();
    });
  });

  it('displays query error state with retry action', async () => {
    const fake = new FakeDesktopTransport(sampleAssets);
    // Make listAssets fail
    fake.listAssets = async () => {
      throw { category: 'unavailable', message: 'Storage is currently busy; retry later.' };
    };
    setTransport(fake);

    render(<App />);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
      expect(screen.getByText('Query Execution Failed')).toBeInTheDocument();
      expect(screen.getByText('Storage is currently busy; retry later.')).toBeInTheDocument();
    });

    // Fix error and click retry
    fake.listAssets = new FakeDesktopTransport(sampleAssets).listAssets;
    const retryBtn = screen.getByRole('button', { name: 'Retry' });
    await userEvent.click(retryBtn);

    await waitFor(() => {
      expect(screen.getByText('3 assets recorded')).toBeInTheDocument();
    });
  });
});
