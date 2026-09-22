import React, { useState, useEffect, useCallback } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type {
  ActivityQuery,
  ActivityViewDto,
  AppCapabilities,
  DesktopError,
} from './types';

interface ActivityFeedProps {
  assetId?: string;
  /// Supplies the asset-kind list for the kind filter. Optional so the feed
  /// still renders before capabilities have loaded — the filter simply has no
  /// options to offer until then.
  capabilities?: AppCapabilities | null;
  onOpenAssetDetail?: (assetId: string) => void;
}

export const ActivityFeed: React.FC<ActivityFeedProps> = ({
  assetId,
  capabilities,
  onOpenAssetDetail,
}) => {
  const [events, setEvents] = useState<ActivityViewDto[]>([]);
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<DesktopError | null>(null);

  // Filters. `kind` is narrower than `module`: a module owns several kinds, so
  // "movie events only" is a question the module filter cannot answer.
  const [moduleFilter, setModuleFilter] = useState<string>('all');
  const [kindFilter, setKindFilter] = useState<string>('all');
  const [eventTypeFilter, setEventTypeFilter] = useState<string>('');
  const [actorFilter, setActorFilter] = useState<string>('');
  const [sinceFilter, setSinceFilter] = useState<string>('');
  const [untilFilter, setUntilFilter] = useState<string>('');

  // Pagination
  const [offset, setOffset] = useState<number>(0);
  const [limit, setLimit] = useState<number>(20);

  // Expanded payloads
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());

  const transport = getTransport();
  const assetKinds = capabilities?.asset_kinds ?? [];

  const loadActivity = useCallback(async () => {
    setLoading(true);
    setError(null);

    const query: ActivityQuery = {
      asset_id: assetId || undefined,
      modules: moduleFilter !== 'all' ? [moduleFilter] : undefined,
      kinds: kindFilter !== 'all' ? [kindFilter] : undefined,
      event_types: eventTypeFilter.trim() ? [eventTypeFilter.trim()] : undefined,
      actors: actorFilter.trim() ? [actorFilter.trim()] : undefined,
      since: sinceFilter.trim() ? sinceFilter.trim() : undefined,
      until: untilFilter.trim() ? untilFilter.trim() : undefined,
      offset,
      limit,
    };

    try {
      const page = await transport.activityQuery(query);
      setEvents(page.items);
      setTotal(page.total);
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setLoading(false);
    }
  }, [assetId, moduleFilter, kindFilter, eventTypeFilter, actorFilter, sinceFilter, untilFilter, offset, limit, transport]);

  useEffect(() => {
    loadActivity();
  }, [loadActivity]);

  const togglePayload = (id: string) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleResetFilters = () => {
    setModuleFilter('all');
    setKindFilter('all');
    setEventTypeFilter('');
    setActorFilter('');
    setSinceFilter('');
    setUntilFilter('');
    setOffset(0);
  };

  return (
    <div
      data-testid="activity-feed-container"
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        backgroundColor: 'var(--color-surface)',
        overflow: 'hidden',
      }}
    >
      {/* Header & Filters */}
      <div
        style={{
          padding: '16px',
          borderBottom: '1px solid var(--color-border)',
          display: 'flex',
          flexDirection: 'column',
          gap: '12px',
          backgroundColor: 'var(--color-canvas)',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
          <div>
            <h2
              style={{
                fontSize: '16px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: 0,
              }}
            >
              {assetId ? 'Asset Activity History' : 'Global Activity Feed'}
            </h2>
            <div style={{ fontSize: '12px', color: 'var(--color-muted)', marginTop: '2px' }}>
              Append-only historical provenance across all asset lifecycle events
            </div>
          </div>
          <button
            type="button"
            data-testid="refresh-activity-button"
            onClick={() => loadActivity()}
            disabled={loading}
            style={{
              padding: '4px 10px',
              fontSize: '12px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              cursor: loading ? 'not-allowed' : 'pointer',
              color: 'var(--color-ink)',
            }}
          >
            {loading ? 'Refreshing...' : '↻ Refresh'}
          </button>
        </div>

        {/* Filter Toolbar */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            flexWrap: 'wrap',
            fontSize: '12px',
          }}
        >
          {/* Module filter */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <label htmlFor="activity-module-select" style={{ color: 'var(--color-muted)' }}>
              Module:
            </label>
            <select
              id="activity-module-select"
              data-testid="activity-module-filter"
              value={moduleFilter}
              onChange={(e) => {
                setModuleFilter(e.target.value);
                setOffset(0);
              }}
              style={{
                padding: '3px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
              }}
            >
              <option value="all">All Modules</option>
              <option value="media">Media</option>
              <option value="software">Software</option>
              <option value="services">Services</option>
              <option value="asset">Asset</option>
              <option value="relation">Relation</option>
              <option value="import">Import</option>
            </select>
          </div>

          {/* Kind filter */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <label htmlFor="activity-kind-select" style={{ color: 'var(--color-muted)' }}>
              Kind:
            </label>
            <select
              id="activity-kind-select"
              data-testid="activity-kind-filter"
              value={kindFilter}
              onChange={(e) => {
                setKindFilter(e.target.value);
                setOffset(0);
              }}
              style={{
                padding: '3px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
              }}
            >
              <option value="all">All Kinds</option>
              {assetKinds.map((kind) => (
                <option key={kind} value={kind}>
                  {kind}
                </option>
              ))}
            </select>
          </div>

          {/* Event type filter */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <label htmlFor="activity-type-input" style={{ color: 'var(--color-muted)' }}>
              Event:
            </label>
            <input
              id="activity-type-input"
              data-testid="activity-event-type-filter"
              type="text"
              placeholder="e.g. asset.created"
              value={eventTypeFilter}
              onChange={(e) => {
                setEventTypeFilter(e.target.value);
                setOffset(0);
              }}
              style={{
                padding: '3px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
                width: '120px',
              }}
            />
          </div>

          {/* Actor filter */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <label htmlFor="activity-actor-input" style={{ color: 'var(--color-muted)' }}>
              Actor:
            </label>
            <input
              id="activity-actor-input"
              data-testid="activity-actor-filter"
              type="text"
              placeholder="e.g. user"
              value={actorFilter}
              onChange={(e) => {
                setActorFilter(e.target.value);
                setOffset(0);
              }}
              style={{
                padding: '3px 8px',
                fontSize: '12px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--color-border)',
                backgroundColor: 'var(--color-surface)',
                color: 'var(--color-ink)',
                width: '80px',
              }}
            />
          </div>

          {(moduleFilter !== 'all' || kindFilter !== 'all' || eventTypeFilter || actorFilter || sinceFilter || untilFilter) && (
            <button
              type="button"
              data-testid="reset-activity-filters"
              onClick={handleResetFilters}
              style={{
                padding: '2px 6px',
                fontSize: '11px',
                background: 'none',
                border: 'none',
                color: 'var(--color-mesh)',
                cursor: 'pointer',
                textDecoration: 'underline',
              }}
            >
              Reset Filters
            </button>
          )}
        </div>
      </div>

      {/* Error state */}
      {error && (
        <div
          data-testid="activity-error-banner"
          style={{
            margin: '12px 16px',
            padding: '10px 14px',
            backgroundColor: 'var(--color-danger-subtle, #ffebee)',
            border: '1px solid var(--color-danger, #d32f2f)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--color-danger, #d32f2f)',
            fontSize: '12px',
          }}
        >
          <strong>Error [{error.category}]:</strong> {error.message}
        </div>
      )}

      {/* Content Feed */}
      <div
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '16px',
        }}
      >
        {loading && events.length === 0 ? (
          <div
            data-testid="activity-loading-indicator"
            style={{ textAlign: 'center', padding: '32px', color: 'var(--color-muted)', fontSize: '13px' }}
          >
            Loading activity history...
          </div>
        ) : events.length === 0 ? (
          <div
            data-testid="activity-empty-state"
            style={{
              textAlign: 'center',
              padding: '48px 16px',
              color: 'var(--color-muted)',
              fontSize: '13px',
              backgroundColor: 'var(--color-canvas)',
              borderRadius: 'var(--radius-md)',
              border: '1px dashed var(--color-border)',
            }}
          >
            No activity events found matching the criteria.
          </div>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            {events.map((event) => {
              const isExpanded = expandedIds.has(event.id);
              return (
                <div
                  key={event.id}
                  data-testid={`activity-event-${event.id}`}
                  style={{
                    backgroundColor: 'var(--color-surface)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    padding: '10px 14px',
                    display: 'flex',
                    flexDirection: 'column',
                    gap: '6px',
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'space-between',
                      flexWrap: 'wrap',
                      gap: '8px',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flexWrap: 'wrap' }}>
                      <span
                        style={{
                          fontSize: '11px',
                          fontFamily: 'var(--font-mono)',
                          color: 'var(--color-muted)',
                        }}
                      >
                        {new Date(event.occurred_at).toLocaleString()}
                      </span>
                      <Badge variant="mesh">{event.event_type}</Badge>
                      {event.module && <Badge variant="muted">{event.module}</Badge>}
                      <span
                        style={{
                          fontSize: '11px',
                          color: 'var(--color-muted)',
                          backgroundColor: 'var(--color-canvas)',
                          padding: '1px 6px',
                          borderRadius: 'var(--radius-sm)',
                        }}
                      >
                        by {event.actor}
                      </span>
                    </div>

                    {event.asset_id && (
                      <div style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                        <span style={{ fontSize: '12px', color: 'var(--color-muted)' }}>Asset:</span>
                        {onOpenAssetDetail ? (
                          <button
                            type="button"
                            data-testid={`activity-asset-link-${event.asset_id}`}
                            onClick={() => onOpenAssetDetail(event.asset_id!)}
                            style={{
                              background: 'none',
                              border: 'none',
                              padding: 0,
                              fontSize: '12px',
                              fontWeight: 600,
                              color: 'var(--color-mesh)',
                              cursor: 'pointer',
                              textDecoration: 'underline',
                            }}
                          >
                            {event.asset_name || event.asset_id}
                          </button>
                        ) : (
                          <span style={{ fontSize: '12px', fontWeight: 600, color: 'var(--color-ink)' }}>
                            {event.asset_name || event.asset_id}
                          </span>
                        )}
                      </div>
                    )}
                  </div>

                  {/* Payload toggle button */}
                  {event.payload && Object.keys(event.payload).length > 0 && (
                    <div style={{ marginTop: '2px' }}>
                      <button
                        type="button"
                        data-testid={`activity-payload-toggle-${event.id}`}
                        onClick={() => togglePayload(event.id)}
                        style={{
                          background: 'none',
                          border: 'none',
                          padding: 0,
                          fontSize: '11px',
                          color: 'var(--color-muted)',
                          cursor: 'pointer',
                          textDecoration: 'underline',
                        }}
                      >
                        {isExpanded ? '▾ Hide Details' : '▸ View Payload'}
                      </button>
                      {isExpanded && (
                        <pre
                          data-testid={`activity-payload-${event.id}`}
                          style={{
                            margin: '6px 0 0',
                            padding: '8px 10px',
                            backgroundColor: 'var(--color-canvas)',
                            border: '1px solid var(--color-border)',
                            borderRadius: 'var(--radius-sm)',
                            fontSize: '11px',
                            fontFamily: 'var(--font-mono)',
                            color: 'var(--color-ink)',
                            overflowX: 'auto',
                            whiteSpace: 'pre-wrap',
                          }}
                        >
                          {JSON.stringify(event.payload, null, 2)}
                        </pre>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Pagination Footer */}
      <div
        style={{
          padding: '10px 16px',
          borderTop: '1px solid var(--color-border)',
          backgroundColor: 'var(--color-canvas)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          fontSize: '12px',
          color: 'var(--color-muted)',
        }}
      >
        <div>
          {total != null ? (
            <span>
              Showing {events.length > 0 ? offset + 1 : 0}–{offset + events.length} of {total}
            </span>
          ) : (
            <span>Showing {events.length} events</span>
          )}
        </div>

        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <label style={{ fontSize: '11px' }}>
            Per Page:
            <select
              value={limit}
              onChange={(e) => {
                setLimit(Number(e.target.value));
                setOffset(0);
              }}
              style={{
                marginLeft: '4px',
                padding: '2px 4px',
                fontSize: '11px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                backgroundColor: 'var(--color-surface)',
              }}
            >
              <option value={10}>10</option>
              <option value={20}>20</option>
              <option value={50}>50</option>
            </select>
          </label>

          <button
            type="button"
            data-testid="activity-prev-page"
            disabled={offset === 0 || loading}
            onClick={() => setOffset(Math.max(0, offset - limit))}
            style={{
              padding: '3px 8px',
              fontSize: '11px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              cursor: offset === 0 || loading ? 'not-allowed' : 'pointer',
            }}
          >
            ← Prev
          </button>
          <button
            type="button"
            data-testid="activity-next-page"
            disabled={(total != null && offset + limit >= total) || events.length < limit || loading}
            onClick={() => setOffset(offset + limit)}
            style={{
              padding: '3px 8px',
              fontSize: '11px',
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              cursor:
                (total != null && offset + limit >= total) || events.length < limit || loading
                  ? 'not-allowed'
                  : 'pointer',
            }}
          >
            Next →
          </button>
        </div>
      </div>
    </div>
  );
};
