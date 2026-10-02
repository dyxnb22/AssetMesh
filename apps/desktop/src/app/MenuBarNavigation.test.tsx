import { act, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setLang } from '../i18n';
import { setTransport } from '../features/library/transport';
import type { AssetDetailDto, AssetSummary } from '../features/library/types';
import { FakeDesktopTransport } from '../test/fake-transport';
import { App } from './App';

const listedAsset: AssetSummary = {
  id: '019315d0-7a00-7000-8000-000000000001',
  kind: 'media.anime',
  name: 'Listed media asset',
  lifecycle: 'active',
  subtitle: null,
  tags: [],
  updated_at: '2026-09-22T10:00:00Z',
};

const serviceDetail: AssetDetailDto = {
  id: '019315d0-7a00-7000-8000-000000000002',
  kind: 'service.local',
  name: 'Unlisted local service',
  summary: null,
  lifecycle: 'active',
  revision: 1,
  created_at: '2026-09-22T09:00:00Z',
  updated_at: '2026-09-22T09:00:00Z',
  archived_at: null,
  merged_into: null,
  tags: [],
  external_refs: [],
  details: {
    module: 'services',
    asset_id: '019315d0-7a00-7000-8000-000000000002',
    service_type: 'local',
    project_dir: '/tmp/unlisted-service',
    start_command: 'npm run dev',
  },
};

describe('Menu bar service navigation', () => {
  beforeEach(() => {
    window.history.replaceState(null, '', '#/');
    setLang('en');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  const renderLibrary = async () => {
    const fake = new FakeDesktopTransport([listedAsset]);
    fake.details.set(serviceDetail.id, serviceDetail);
    setTransport(fake);
    const view = render(<App />);
    await screen.findByRole('row', { name: /Listed media asset/ });
    return { fake, ...view };
  };

  it('opens a service detail by ID even when it is absent from the current list', async () => {
    const { fake } = await renderLibrary();
    const getAsset = vi.spyOn(fake, 'getAsset');
    expect(screen.queryByText(serviceDetail.name)).not.toBeInTheDocument();

    act(() => {
      window.dispatchEvent(new CustomEvent('assetmesh-open-service', { detail: serviceDetail.id }));
    });

    const dialog = await screen.findByRole('dialog', { name: 'Asset Detail View' });
    expect(await within(dialog).findByRole('heading', { name: serviceDetail.name })).toBeInTheDocument();
    expect(within(dialog).getByTestId('service-panel')).toHaveTextContent('/tmp/unlisted-service');
    expect(getAsset).toHaveBeenCalledWith(serviceDetail.id);
    expect(screen.getByRole('row', { name: /Listed media asset/ })).toBeInTheDocument();
  });

  it('opens the detail when the native menu also navigates to the services hash', async () => {
    await renderLibrary();

    act(() => {
      window.location.hash = `#/?module=services&asset=${serviceDetail.id}`;
      window.dispatchEvent(new CustomEvent('assetmesh-open-service', { detail: serviceDetail.id }));
    });

    const dialog = await screen.findByRole('dialog', { name: 'Asset Detail View' });
    expect(await within(dialog).findByRole('heading', { name: serviceDetail.name })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('tab', { name: 'Services' })).toHaveAttribute('aria-selected', 'true'));
  });

  it('dismisses an open detail when the native menu selects Manage services', async () => {
    await renderLibrary();
    act(() => {
      window.dispatchEvent(new CustomEvent('assetmesh-open-service', { detail: serviceDetail.id }));
    });
    const dialog = await screen.findByRole('dialog', { name: 'Asset Detail View' });
    await within(dialog).findByRole('heading', { name: serviceDetail.name });

    act(() => {
      window.location.hash = '#/?module=services';
      window.dispatchEvent(new Event('assetmesh-manage-services'));
    });

    expect(screen.queryByRole('dialog', { name: 'Asset Detail View' })).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole('tab', { name: 'Services' })).toHaveAttribute('aria-selected', 'true'));
  });

  it('ignores events with missing, non-string, or malformed IDs', async () => {
    const { fake } = await renderLibrary();
    const getAsset = vi.spyOn(fake, 'getAsset');

    act(() => {
      window.dispatchEvent(new Event('assetmesh-open-service'));
      for (const detail of [undefined, null, 42, { asset_id: serviceDetail.id }, '', 'not-a-uuid', `${serviceDetail.id}x`, ` ${serviceDetail.id}`, serviceDetail.id.replace(/-/g, '')]) {
        window.dispatchEvent(new CustomEvent('assetmesh-open-service', { detail }));
      }
    });

    expect(screen.queryByRole('dialog', { name: 'Asset Detail View' })).not.toBeInTheDocument();
    expect(getAsset).not.toHaveBeenCalled();
    expect(screen.getByRole('row', { name: /Listed media asset/ })).toBeInTheDocument();
  });

  it('removes the native navigation listeners when the app unmounts', async () => {
    const addListener = vi.spyOn(window, 'addEventListener');
    const removeListener = vi.spyOn(window, 'removeEventListener');
    const { fake, unmount } = await renderLibrary();
    const getAsset = vi.spyOn(fake, 'getAsset');
    const registration = addListener.mock.calls.find(([name]) => name === 'assetmesh-open-service');
    const manageRegistration = addListener.mock.calls.find(([name]) => name === 'assetmesh-manage-services');
    expect(registration).toBeDefined();
    expect(manageRegistration).toBeDefined();

    unmount();

    expect(removeListener).toHaveBeenCalledWith('assetmesh-open-service', registration?.[1]);
    expect(removeListener).toHaveBeenCalledWith('assetmesh-manage-services', manageRegistration?.[1]);
    act(() => {
      window.dispatchEvent(new CustomEvent('assetmesh-open-service', { detail: serviceDetail.id }));
    });
    expect(getAsset).not.toHaveBeenCalled();
  });
});
