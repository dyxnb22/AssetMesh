import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { App } from '../../app/App';
import { RelationExplorer } from './RelationExplorer';
import { setTransport } from './transport';
import { FakeDesktopTransport } from '../../test/fake-transport';
import type { AssetSummary } from './types';

const assetA: AssetSummary = {
  id: 'asset-a',
  kind: 'software.app',
  name: 'App Alpha',
  lifecycle: 'active',
  subtitle: 'Primary application',
  tags: ['app'],
  revision: 1,
  updated_at: '2024-03-20T10:00:00Z',
};

const assetB: AssetSummary = {
  id: 'asset-b',
  kind: 'software.tool',
  name: 'Tool Beta',
  lifecycle: 'active',
  subtitle: 'CLI Utility',
  tags: ['cli'],
  revision: 1,
  updated_at: '2024-03-20T10:00:00Z',
};

const assetC: AssetSummary = {
  id: 'asset-c',
  kind: 'service.vps',
  name: 'VPS Gamma',
  lifecycle: 'active',
  subtitle: 'Cloud Server',
  tags: ['infra'],
  revision: 1,
  updated_at: '2024-03-20T10:00:00Z',
};

describe('Relations and Impact Explorer Workflow UI (P5-07)', () => {
  let fakeTransport: FakeDesktopTransport;

  beforeEach(() => {
    fakeTransport = new FakeDesktopTransport([
      JSON.parse(JSON.stringify(assetA)),
      JSON.parse(JSON.stringify(assetB)),
      JSON.parse(JSON.stringify(assetC)),
    ]);
    setTransport(fakeTransport);
  });

  it('Flow 1 (Neighbors List & Direction Filter): lists direct relations and filters by direction', async () => {
    // Attach A -> B (depends_on) and C -> A (depends_on)
    await fakeTransport.relationAttach({
      source_asset_id: assetA.id,
      relation_type: 'depends_on',
      target_asset_id: assetB.id,
    });
    await fakeTransport.relationAttach({
      source_asset_id: assetC.id,
      relation_type: 'depends_on',
      target_asset_id: assetA.id,
    });

    render(
      <RelationExplorer
        rootAsset={assetA}
        capabilities={fakeTransport.capabilities}
      />
    );

    // Default: 'both' direction in 'neighbors' mode
    await screen.findByTestId('neighbors-list-container');

    // Should see both Tool Beta (outgoing) and VPS Gamma (incoming)
    expect(screen.getByText('Tool Beta')).toBeInTheDocument();
    expect(screen.getByText('VPS Gamma')).toBeInTheDocument();
    expect(screen.getByText('→ depends_on')).toBeInTheDocument();
    expect(screen.getByText('← dependency_of')).toBeInTheDocument();

    // Filter to Outgoing only
    const directionSelect = screen.getByTestId('relation-direction-select');
    fireEvent.change(directionSelect, { target: { value: 'outgoing' } });

    await waitFor(() => {
      expect(screen.getByText('Tool Beta')).toBeInTheDocument();
      expect(screen.queryByText('VPS Gamma')).not.toBeInTheDocument();
    });

    // Filter to Incoming only
    fireEvent.change(directionSelect, { target: { value: 'incoming' } });

    await waitFor(() => {
      expect(screen.queryByText('Tool Beta')).not.toBeInTheDocument();
      expect(screen.getByText('VPS Gamma')).toBeInTheDocument();
    });
  });

  it('Flow 2 (Impact & Dependency Path Explorer): explores impact and dependencies with depth and path evidence', async () => {
    // Chain: A depends_on B, B depends_on C
    await fakeTransport.relationAttach({
      source_asset_id: assetA.id,
      relation_type: 'depends_on',
      target_asset_id: assetB.id,
    });
    await fakeTransport.relationAttach({
      source_asset_id: assetB.id,
      relation_type: 'depends_on',
      target_asset_id: assetC.id,
    });

    render(
      <RelationExplorer
        rootAsset={assetA}
        capabilities={fakeTransport.capabilities}
      />
    );

    await screen.findByTestId('neighbors-list-container');

    // Switch mode to Dependencies
    const modeSelect = screen.getByTestId('relation-mode-select');
    fireEvent.change(modeSelect, { target: { value: 'dependencies' } });

    // Should render traversal container
    await screen.findByTestId('traversal-container');

    // Should list Tool Beta at depth 1 and VPS Gamma at depth 2
    const nodeB = screen.getByTestId(`traversal-node-${assetB.id}`);
    expect(nodeB).toHaveTextContent('Depth 1');
    expect(nodeB).toHaveTextContent('Tool Beta');

    const nodeC = screen.getByTestId(`traversal-node-${assetC.id}`);
    expect(nodeC).toHaveTextContent('Depth 2');
    expect(nodeC).toHaveTextContent('VPS Gamma');

    // Path evidence is visible
    expect(screen.getByTestId(`path-evidence-${assetC.id}`)).toHaveTextContent('Path:App Alpha');
  });

  it('Flow 3 (Add Relation via Modal): opens modal, selects target, and attaches relation', async () => {
    const attachSpy = vi.spyOn(fakeTransport, 'relationAttach');

    render(
      <RelationExplorer
        rootAsset={assetA}
        capabilities={fakeTransport.capabilities}
      />
    );

    await screen.findByTestId('neighbors-list-container');

    // Click "+ Add Relation" button
    const addBtn = screen.getByTestId('add-relation-button');
    fireEvent.click(addBtn);

    // Modal opens
    await screen.findByTestId('attach-relation-form');

    // Select relation type "uses"
    const typeSelect = screen.getByTestId('attach-relation-type-select');
    fireEvent.change(typeSelect, { target: { value: 'uses' } });

    // Target option appears in search results
    const targetOption = await screen.findByTestId(`target-asset-option-${assetB.id}`);
    fireEvent.click(targetOption);

    // Selected target card appears
    await screen.findByTestId('selected-target-card');

    // Add note
    const noteInput = screen.getByTestId('attach-relation-note-input');
    fireEvent.change(noteInput, { target: { value: 'Uses CLI utility for compilation' } });

    // Submit form
    fireEvent.click(screen.getByTestId('submit-attach-relation-button'));

    await waitFor(() => {
      expect(attachSpy).toHaveBeenCalledWith({
        source_asset_id: assetA.id,
        relation_type: 'uses',
        target_asset_id: assetB.id,
        note: 'Uses CLI utility for compilation',
        expected_source_revision: 1,
        expected_target_revision: 1,
      });
    });

    // Modal closes and neighbor is rendered
    await waitFor(() => {
      expect(screen.queryByTestId('attach-relation-form')).not.toBeInTheDocument();
      expect(screen.getByText('Tool Beta')).toBeInTheDocument();
      expect(screen.getByText('→ uses')).toBeInTheDocument();
    });
  });

  it('Flow 4 (Remove Relation with Confirmation): prompts confirmation and removes relation', async () => {
    const removeSpy = vi.spyOn(fakeTransport, 'relationRemove');

    const receipt = await fakeTransport.relationAttach({
      source_asset_id: assetA.id,
      relation_type: 'depends_on',
      target_asset_id: assetB.id,
    });
    expect(receipt.asset_ids).toContain(assetA.id);
    const attachedRelId = 'rel-1';

    render(
      <RelationExplorer
        rootAsset={fakeTransport.assets.find((a) => a.id === assetA.id)!}
        capabilities={fakeTransport.capabilities}
      />
    );

    await screen.findByTestId('neighbors-list-container');
    expect(screen.getByText('Tool Beta')).toBeInTheDocument();

    // Click Remove button
    const removeBtn = screen.getByTestId(`remove-relation-${attachedRelId}`);
    fireEvent.click(removeBtn);

    // Confirmation box appears
    await screen.findByTestId('remove-confirm-box');

    // Confirm removal
    const confirmBtn = screen.getByTestId('confirm-remove-relation-button');
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(removeSpy).toHaveBeenCalledWith({
        relation_id: attachedRelId,
        context_asset_id: assetA.id,
        expected_context_revision: 2,
      });
    });

    // Neighbor is removed from list
    await waitFor(() => {
      expect(screen.queryByText('Tool Beta')).not.toBeInTheDocument();
    });
  });

  it('Flow 5 (Graph View / List View Toggle): switches between visual SVG graph and accessible list', async () => {
    await fakeTransport.relationAttach({
      source_asset_id: assetA.id,
      relation_type: 'depends_on',
      target_asset_id: assetB.id,
    });

    render(
      <RelationExplorer
        rootAsset={assetA}
        capabilities={fakeTransport.capabilities}
      />
    );

    // Switch to dependencies mode
    const modeSelect = screen.getByTestId('relation-mode-select');
    fireEvent.change(modeSelect, { target: { value: 'dependencies' } });

    await screen.findByTestId('traversal-container');

    // Default viewMode is 'list'
    expect(screen.getByTestId('traversal-list-view')).toBeInTheDocument();
    expect(screen.queryByTestId('traversal-graph-view')).not.toBeInTheDocument();

    // Toggle to Graph view
    const graphToggleBtn = screen.getByTestId('toggle-graph-view');
    fireEvent.click(graphToggleBtn);

    // SVG graph rendered
    expect(screen.getByTestId('traversal-graph-view')).toBeInTheDocument();
    expect(screen.queryByTestId('traversal-list-view')).not.toBeInTheDocument();

    // Toggle back to List view
    const listToggleBtn = screen.getByTestId('toggle-list-view');
    fireEvent.click(listToggleBtn);

    expect(screen.getByTestId('traversal-list-view')).toBeInTheDocument();
    expect(screen.queryByTestId('traversal-graph-view')).not.toBeInTheDocument();
  });

  it('Flow 6 (Navigation Rail - Relations Workspace in App): navigates to relations section in main shell', async () => {
    render(<App />);

    // Wait for ledger
    await screen.findAllByText('App Alpha');

    // Click Relations in rail
    const relNavBtn = await screen.findByTestId('nav-relations');
    fireEvent.click(relNavBtn);

    // Relations workspace mounts
    const workspace = await screen.findByTestId('relations-workspace');
    expect(workspace).toBeInTheDocument();

    // Explorer header is visible for selected asset
    expect(screen.getByTestId('relation-explorer')).toBeInTheDocument();
    expect(screen.getByText('App Alpha')).toBeInTheDocument();
  });
});
