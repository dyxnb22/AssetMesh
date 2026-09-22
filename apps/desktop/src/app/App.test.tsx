import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { setTransport } from '../features/library/transport';
import type { AssetSummary } from '../features/library/types';
import { FakeDesktopTransport } from '../test/fake-transport';
import { App } from './App';

const sampleAsset: AssetSummary = {
  id: '019315d0-7a00-7000-8000-000000000001',
  kind: 'media.anime',
  name: 'Sousou no Frieren',
  lifecycle: 'active',
  subtitle: 'TV Series · 28 episodes',
  tags: ['healing', 'fantasy'],
  updated_at: '2026-09-22T10:00:00Z',
};

describe('App Desktop Shell', () => {
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

  it('renders ready state with real asset summary row and updates inspector on click', async () => {
    const fake = new FakeDesktopTransport([sampleAsset]);
    setTransport(fake);

    render(<App />);

    // Check All Assets header and item count
    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'All Assets' })).toBeInTheDocument();
      expect(screen.getByText('1 items recorded')).toBeInTheDocument();
    });

    // Check row content and inspector content
    const row = screen.getByRole('row');
    expect(row).toHaveTextContent('Sousou no Frieren');
    expect(row).toHaveTextContent('TV Series · 28 episodes');
    expect(row).toHaveTextContent('#healing');

    // Check inspector reflects the asset details
    expect(screen.getByRole('complementary', { name: 'Asset Inspector' })).toBeInTheDocument();
    expect(screen.getByText('019315d0-7a00-7000-8000-000000000001')).toBeInTheDocument();

    // Keyboard interaction
    await userEvent.click(row);
    expect(row).toHaveStyle({ cursor: 'pointer' });
  });
});
