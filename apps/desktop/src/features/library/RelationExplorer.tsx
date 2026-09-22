import React, { useState, useEffect, useCallback } from 'react';
import { Badge } from '../../ui/Badge';
import { AttachRelationModal } from './AttachRelationModal';
import { getTransport, normalizeDesktopError } from './transport';
import type {
  AppCapabilities,
  DesktopError,
  NeighborViewDto,
  TraversalViewDto,
} from './types';

interface RelationExplorerProps {
  rootAsset: { id: string; name: string; kind?: string; lifecycle?: string };
  capabilities: AppCapabilities | null;
  onOpenAssetDetail?: (assetId: string) => void;
  onAssetUpdated?: () => void;
}

export const RelationExplorer: React.FC<RelationExplorerProps> = ({
  rootAsset,
  capabilities,
  onOpenAssetDetail,
  onAssetUpdated,
}) => {
  const [mode, setMode] = useState<
    'neighbors' | 'impact' | 'dependencies' | 'dependents' | 'traverse'
  >('neighbors');
  const [direction, setDirection] = useState<'both' | 'outgoing' | 'incoming'>('both');
  const [maxDepth, setMaxDepth] = useState<number>(8);
  const [includeArchived, setIncludeArchived] = useState<boolean>(false);
  const [viewMode, setViewMode] = useState<'list' | 'graph'>('list');

  const [neighbors, setNeighbors] = useState<NeighborViewDto[] | null>(null);
  const [traversal, setTraversal] = useState<TraversalViewDto | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<DesktopError | null>(null);

  // Modals
  const [isAttachOpen, setIsAttachOpen] = useState(false);
  const [deletingRelationId, setDeletingRelationId] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

  const transport = getTransport();

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      if (mode === 'neighbors') {
        const results = await transport.relationNeighbors({
          asset_id: rootAsset.id,
          direction,
          include_archived: includeArchived,
        });
        setNeighbors(results);
        setTraversal(null);
      } else {
        const results = await transport.relationTraverse({
          asset_id: rootAsset.id,
          mode,
          direction: mode === 'traverse' ? direction : undefined,
          max_depth: maxDepth,
          include_archived: includeArchived,
        });
        setTraversal(results);
        setNeighbors(null);
      }
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setLoading(false);
    }
  }, [direction, includeArchived, maxDepth, mode, rootAsset.id, transport]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleRemoveRelation = async (relationId: string) => {
    setDeleting(true);
    try {
      await transport.relationRemove({ relation_id: relationId });
      setDeletingRelationId(null);
      loadData();
      onAssetUpdated?.();
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div
      data-testid="relation-explorer"
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '14px',
        backgroundColor: 'var(--color-surface)',
        border: '1px solid var(--color-border)',
        borderRadius: 'var(--radius-md)',
        padding: '16px',
      }}
    >
      {/* Header with Mode Tabs & Add Button */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: '10px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
            Relations & Impact Explorer
          </span>
          <Badge variant="mesh">{rootAsset.name}</Badge>
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          {rootAsset.lifecycle !== 'archived' && (
            <button
              type="button"
              data-testid="add-relation-button"
              onClick={() => setIsAttachOpen(true)}
              style={{
                backgroundColor: 'var(--color-mesh)',
                color: '#fff',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                padding: '4px 10px',
                fontSize: '12px',
                fontWeight: 500,
                cursor: 'pointer',
              }}
            >
              + Add Relation
            </button>
          )}

          <div
            style={{
              display: 'flex',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              overflow: 'hidden',
            }}
          >
            <button
              type="button"
              data-testid="toggle-list-view"
              onClick={() => setViewMode('list')}
              aria-pressed={viewMode === 'list'}
              style={{
                padding: '4px 8px',
                fontSize: '11px',
                border: 'none',
                backgroundColor: viewMode === 'list' ? 'var(--color-mesh-bg)' : 'transparent',
                color: viewMode === 'list' ? 'var(--color-mesh)' : 'var(--color-muted)',
                cursor: 'pointer',
                fontWeight: viewMode === 'list' ? 600 : 400,
              }}
            >
              ≡ List
            </button>
            <button
              type="button"
              data-testid="toggle-graph-view"
              onClick={() => setViewMode('graph')}
              aria-pressed={viewMode === 'graph'}
              style={{
                padding: '4px 8px',
                fontSize: '11px',
                border: 'none',
                backgroundColor: viewMode === 'graph' ? 'var(--color-mesh-bg)' : 'transparent',
                color: viewMode === 'graph' ? 'var(--color-mesh)' : 'var(--color-muted)',
                cursor: 'pointer',
                fontWeight: viewMode === 'graph' ? 600 : 400,
              }}
            >
              ☊ Graph
            </button>
          </div>
        </div>
      </div>

      {/* Mode Controls Bar */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: '8px',
          padding: '8px 12px',
          backgroundColor: 'var(--color-canvas)',
          borderRadius: 'var(--radius-sm)',
          fontSize: '12px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          <span style={{ color: 'var(--color-muted)', fontWeight: 500 }}>Mode:</span>
          <select
            data-testid="relation-mode-select"
            value={mode}
            onChange={(e) =>
              setMode(
                e.target.value as
                  | 'neighbors'
                  | 'impact'
                  | 'dependencies'
                  | 'dependents'
                  | 'traverse'
              )
            }
            style={{
              padding: '3px 8px',
              fontSize: '12px',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid var(--color-border)',
              backgroundColor: 'var(--color-surface)',
            }}
          >
            <option value="neighbors">Direct Neighbors</option>
            <option value="impact">Impact Analysis</option>
            <option value="dependencies">Dependencies (Outgoing)</option>
            <option value="dependents">Dependents (Incoming)</option>
            <option value="traverse">Full Traversal</option>
          </select>
        </div>

        {(mode === 'neighbors' || mode === 'traverse') && (
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-muted)' }}>Direction:</span>
            <select
              data-testid="relation-direction-select"
              value={direction}
              onChange={(e) =>
                setDirection(e.target.value as 'both' | 'outgoing' | 'incoming')
              }
              style={{
                padding: '3px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
              }}
            >
              <option value="both">Both</option>
              <option value="outgoing">Outgoing</option>
              <option value="incoming">Incoming</option>
            </select>
          </div>
        )}

        {mode !== 'neighbors' && (
          <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
            <span style={{ color: 'var(--color-muted)' }}>Max Depth:</span>
            <input
              type="number"
              data-testid="relation-depth-input"
              min={1}
              max={32}
              value={maxDepth}
              onChange={(e) => setMaxDepth(Math.max(1, Math.min(32, Number(e.target.value) || 1)))}
              style={{
                width: '45px',
                padding: '2px 4px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
              }}
            />
          </div>
        )}

        <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          <input
            type="checkbox"
            id="include-archived-rel"
            data-testid="include-archived-checkbox"
            checked={includeArchived}
            onChange={(e) => setIncludeArchived(e.target.checked)}
          />
          <label htmlFor="include-archived-rel" style={{ cursor: 'pointer', color: 'var(--color-ink)' }}>
            Include Archived
          </label>
        </div>
      </div>

      {/* Error state */}
      {error && (
        <div
          role="alert"
          data-testid="relation-error-banner"
          style={{
            padding: '10px 14px',
            backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-md)',
            color: 'var(--color-danger)',
            fontSize: '12px',
          }}
        >
          <div style={{ fontWeight: 600 }}>{error.category}</div>
          <div>{error.message}</div>
        </div>
      )}

      {/* Loading state */}
      {loading && (
        <div style={{ padding: '20px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '13px' }}>
          Loading relations...
        </div>
      )}

      {/* Content Area */}
      {!loading && !error && (
        <>
          {/* 1. Neighbors Mode Content */}
          {mode === 'neighbors' && neighbors && (
            <div data-testid="neighbors-list-container">
              {neighbors.length === 0 ? (
                <div style={{ padding: '16px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '12px' }}>
                  No relations found matching the criteria.
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                  {neighbors.map((n) => (
                    <div
                      key={n.edge.relation_id}
                      data-testid={`neighbor-row-${n.edge.relation_id}`}
                      style={{
                        padding: '10px 12px',
                        border: '1px solid var(--color-border)',
                        borderRadius: 'var(--radius-sm)',
                        backgroundColor: 'var(--color-surface)',
                        display: 'flex',
                        alignItems: 'center',
                        justifyContent: 'space-between',
                        gap: '12px',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
                        <Badge variant={n.edge.outgoing ? 'mesh' : 'muted'}>
                          {n.edge.outgoing ? '→ ' + n.edge.relation_type : '← ' + n.edge.relation_type}
                        </Badge>

                        <button
                          type="button"
                          onClick={() => onOpenAssetDetail?.(n.asset.id)}
                          style={{
                            background: 'none',
                            border: 'none',
                            cursor: 'pointer',
                            fontWeight: 600,
                            color: 'var(--color-ink)',
                            fontSize: '13px',
                            padding: 0,
                          }}
                        >
                          {n.asset.name}
                        </button>

                        <Badge variant="muted">{n.asset.kind}</Badge>
                        {n.asset.lifecycle !== 'active' && (
                          <Badge variant="attention">{n.asset.lifecycle}</Badge>
                        )}
                        {n.edge.note && (
                          <span style={{ fontSize: '12px', color: 'var(--color-muted)' }}>
                            ({n.edge.note})
                          </span>
                        )}
                      </div>

                      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        {deletingRelationId === n.edge.relation_id ? (
                          <div
                            data-testid="remove-confirm-box"
                            style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
                          >
                            <span style={{ fontSize: '11px', color: 'var(--color-danger)' }}>
                              Confirm?
                            </span>
                            <button
                              type="button"
                              data-testid="confirm-remove-relation-button"
                              disabled={deleting}
                              onClick={() => handleRemoveRelation(n.edge.relation_id)}
                              style={{
                                padding: '2px 8px',
                                backgroundColor: 'var(--color-danger)',
                                color: '#fff',
                                border: 'none',
                                borderRadius: 'var(--radius-sm)',
                                fontSize: '11px',
                                cursor: deleting ? 'not-allowed' : 'pointer',
                              }}
                            >
                              Yes
                            </button>
                            <button
                              type="button"
                              onClick={() => setDeletingRelationId(null)}
                              disabled={deleting}
                              style={{
                                padding: '2px 8px',
                                backgroundColor: 'var(--color-canvas)',
                                border: '1px solid var(--color-border)',
                                borderRadius: 'var(--radius-sm)',
                                fontSize: '11px',
                                cursor: 'pointer',
                              }}
                            >
                              No
                            </button>
                          </div>
                        ) : (
                          rootAsset.lifecycle !== 'archived' && (
                            <button
                              type="button"
                              data-testid={`remove-relation-${n.edge.relation_id}`}
                              onClick={() => setDeletingRelationId(n.edge.relation_id)}
                              style={{
                                background: 'none',
                                border: '1px solid var(--color-border)',
                                borderRadius: 'var(--radius-sm)',
                                padding: '2px 8px',
                                fontSize: '11px',
                                color: 'var(--color-muted)',
                                cursor: 'pointer',
                              }}
                            >
                              Remove
                            </button>
                          )
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* 2. Traversal / Impact Mode Content */}
          {mode !== 'neighbors' && traversal && (
            <div data-testid="traversal-container">
              {/* Summary Stats & Truncation Banner */}
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  marginBottom: '10px',
                  fontSize: '12px',
                  color: 'var(--color-muted)',
                }}
              >
                <div>
                  <strong>{traversal.nodes.length}</strong> reachable assets found
                </div>
                {traversal.truncated && (
                  <Badge variant="attention">
                    Depth limit ({maxDepth}) reached; deeper nodes omitted
                  </Badge>
                )}
              </div>

              {traversal.nodes.length === 0 ? (
                <div style={{ padding: '16px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '12px' }}>
                  No transitive {mode === 'impact' ? 'dependents' : mode} found.
                </div>
              ) : viewMode === 'list' ? (
                /* Accessible List View */
                <div
                  data-testid="traversal-list-view"
                  style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}
                >
                  {traversal.nodes.map((node) => (
                    <div
                      key={node.asset.id}
                      data-testid={`traversal-node-${node.asset.id}`}
                      style={{
                        padding: '10px 14px',
                        border: '1px solid var(--color-border)',
                        borderRadius: 'var(--radius-sm)',
                        backgroundColor: 'var(--color-surface)',
                        display: 'flex',
                        flexDirection: 'column',
                        gap: '6px',
                      }}
                    >
                      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <span
                            style={{
                              fontSize: '11px',
                              fontFamily: 'var(--font-mono)',
                              backgroundColor: 'var(--color-canvas)',
                              padding: '2px 6px',
                              borderRadius: 'var(--radius-sm)',
                              fontWeight: 600,
                            }}
                          >
                            Depth {node.depth}
                          </span>

                          <button
                            type="button"
                            onClick={() => onOpenAssetDetail?.(node.asset.id)}
                            style={{
                              background: 'none',
                              border: 'none',
                              cursor: 'pointer',
                              fontWeight: 600,
                              color: 'var(--color-ink)',
                              fontSize: '13px',
                              padding: 0,
                            }}
                          >
                            {node.asset.name}
                          </button>

                          <Badge variant="muted">{node.asset.kind}</Badge>
                          {node.asset.lifecycle !== 'active' && (
                            <Badge variant="attention">{node.asset.lifecycle}</Badge>
                          )}
                        </div>
                      </div>

                      {/* Path Explanation Evidence */}
                      <div
                        data-testid={`path-evidence-${node.asset.id}`}
                        style={{
                          fontSize: '11px',
                          fontFamily: 'var(--font-mono)',
                          color: 'var(--color-muted)',
                          display: 'flex',
                          alignItems: 'center',
                          flexWrap: 'wrap',
                          gap: '4px',
                          padding: '4px 8px',
                          backgroundColor: 'var(--color-canvas)',
                          borderRadius: 'var(--radius-sm)',
                        }}
                      >
                        <span style={{ fontWeight: 600 }}>Path:</span>
                        <span>{rootAsset.name}</span>
                        {node.path.map((hop, i) => (
                          <React.Fragment key={i}>
                            <span style={{ color: 'var(--color-mesh)' }}>
                              ──[{hop.relation_type}]──▶
                            </span>
                            <span>{i === node.path.length - 1 ? node.asset.name : hop.to_asset_id.slice(0, 8)}</span>
                          </React.Fragment>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                /* Interactive SVG Graph View (Layout coordinates computed only in UI) */
                <div
                  data-testid="traversal-graph-view"
                  style={{
                    width: '100%',
                    height: '320px',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    backgroundColor: 'var(--color-canvas)',
                    overflow: 'hidden',
                    position: 'relative',
                  }}
                >
                  <svg
                    width="100%"
                    height="100%"
                    viewBox="0 0 600 320"
                    style={{ display: 'block' }}
                  >
                    <defs>
                      <marker
                        id="arrow"
                        viewBox="0 0 10 10"
                        refX="18"
                        refY="5"
                        markerWidth="6"
                        markerHeight="6"
                        orient="auto-start-reverse"
                      >
                        <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--color-mesh)" />
                      </marker>
                    </defs>

                    {/* Root Node at Center */}
                    <g transform="translate(100, 160)">
                      <circle r="22" fill="var(--color-mesh)" />
                      <text
                        textAnchor="middle"
                        dy=".3em"
                        fill="#fff"
                        fontSize="10"
                        fontWeight="bold"
                      >
                        ROOT
                      </text>
                      <text
                        textAnchor="middle"
                        y="34"
                        fill="var(--color-ink)"
                        fontSize="11"
                        fontWeight="600"
                      >
                        {rootAsset.name}
                      </text>
                    </g>

                    {/* Nodes grouped by depth in layered columns */}
                    {traversal.nodes.map((node, idx) => {
                      const count = traversal.nodes.length;
                      const x = 240 + (node.depth - 1) * 160;
                      const stepY = count > 1 ? 260 / Math.max(1, count - 1) : 0;
                      const y = count === 1 ? 160 : 30 + idx * stepY;

                      return (
                        <g key={node.asset.id} transform={`translate(${x}, ${y})`}>
                          {/* Directed Edge from prev hop / root */}
                          <path
                            d={`M -120 ${160 - y} Q -60 0, -20 0`}
                            fill="none"
                            stroke="var(--color-mesh)"
                            strokeWidth="1.5"
                            markerEnd="url(#arrow)"
                          />
                          <circle
                            r="18"
                            fill="var(--color-surface)"
                            stroke="var(--color-border)"
                            strokeWidth="2"
                            style={{ cursor: 'pointer' }}
                            onClick={() => onOpenAssetDetail?.(node.asset.id)}
                          />
                          <text
                            textAnchor="middle"
                            dy=".3em"
                            fill="var(--color-ink)"
                            fontSize="9"
                            fontWeight="bold"
                          >
                            D{node.depth}
                          </text>
                          <text
                            textAnchor="middle"
                            y="28"
                            fill="var(--color-ink)"
                            fontSize="11"
                            fontWeight="500"
                            style={{ cursor: 'pointer' }}
                            onClick={() => onOpenAssetDetail?.(node.asset.id)}
                          >
                            {node.asset.name}
                          </text>
                        </g>
                      );
                    })}
                  </svg>
                </div>
              )}
            </div>
          )}
        </>
      )}

      {/* Attach Relation Modal */}
      <AttachRelationModal
        isOpen={isAttachOpen}
        onClose={() => setIsAttachOpen(false)}
        sourceAsset={{ id: rootAsset.id, name: rootAsset.name }}
        capabilities={capabilities}
        onAttached={() => {
          loadData();
          onAssetUpdated?.();
        }}
      />
    </div>
  );
};
