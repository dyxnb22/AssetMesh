import React, { useEffect, useRef, useState } from 'react';
import { useServiceRuntime } from './useServiceRuntime';
import { CreateLocalServiceModal } from './CreateLocalServiceModal';
import { Icon } from '../../ui/Icon';
import { formatRelativeTime, t } from '../../i18n';
import { getTransport, normalizeDesktopError } from './transport';
import type { AssetSummary, DesktopError, Page, ServiceRecordDto, ServiceRuntimeState } from './types';
import './ServicesWorkspace.css';

interface ServicesWorkspaceProps {
  data: Page<AssetSummary> | null;
  error: DesktopError | null;
  loading: boolean;
  searchQuery: string;
  page: number;
  pageSize: number;
  selectedAssetId: string | null;
  runtimeSupported: boolean;
  onNewAsset: () => void;
  onSelectAsset: (id: string) => void;
  onSearchChange: (search: string) => void;
  onSelectPage: (page: number) => void;
  onResetFilters: () => void;
  onRetry: () => void;
  onOpenDetail: (id: string) => void;
}

function recordOf(asset: AssetSummary): ServiceRecordDto | null {
  return asset.details?.module === 'services' ? asset.details : null;
}
const isRunning = (state?: ServiceRuntimeState) => state === 'running' || state === 'external';
type StatusFilter = 'all' | 'running' | 'stopped';
type Operation = 'start' | 'stop' | 'restart';
function logTime(timestamp: string) {
  const date = new Date(timestamp);
  return Number.isNaN(date.getTime()) ? timestamp : date.toLocaleTimeString(undefined, { hour12: false });
}

export const ServicesWorkspace: React.FC<ServicesWorkspaceProps> = ({
  data, error, loading, searchQuery, page, pageSize, selectedAssetId,
  runtimeSupported, onNewAsset, onSelectAsset, onSearchChange,
  onSelectPage, onResetFilters, onRetry, onOpenDetail,
}) => {
  const transport = getTransport();
  const { statuses, logs, logsDropped, refresh, statusError } = useServiceRuntime(selectedAssetId, runtimeSupported);
  const [filter, setFilter] = useState<StatusFilter>('all');
  const [pending, setPending] = useState<{ id: string; operation: Operation } | null>(null);
  const operationRef = useRef(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const [actionError, setActionError] = useState<{ id: string; message: string } | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [copied, setCopied] = useState<'address' | 'logs' | null>(null);
  const [expandedLogs, setExpandedLogs] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout>>();
  const stateOf = (asset: AssetSummary) => statuses[asset.id]?.state;
  const items = data?.items ?? [];
  const visibleItems = items.filter((asset) => filter === 'all'
    || (filter === 'running' ? isRunning(stateOf(asset)) : ['stopped', 'failed'].includes(stateOf(asset) ?? '')));
  const selected = items.find((item) => item.id === selectedAssetId) ?? null;
  const record = selected ? recordOf(selected) : null;
  const total = data?.total ?? items.length;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const filtered = Boolean(searchQuery.trim()) || page > 1 || filter !== 'all';

  useEffect(() => {
    setExpandedLogs(false);
    setCopied(null);
  }, [selectedAssetId]);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if (document.querySelector('[aria-modal="true"]')) return;
      if ((event.metaKey || event.ctrlKey) && ['f', 'k'].includes(event.key.toLowerCase())) {
        event.preventDefault(); searchRef.current?.focus(); searchRef.current?.select();
      }
    };
    window.addEventListener('keydown', focusSearch);
    return () => window.removeEventListener('keydown', focusSearch);
  }, []);

  const run = async (asset: AssetSummary, operation: Operation) => {
    if (operationRef.current) return;
    operationRef.current = true;
    setPending({ id: asset.id, operation });
    setActionError(null);
    try {
      if (operation === 'start') await transport.serviceRuntimeStart(asset.id);
      else if (operation === 'stop') await transport.serviceRuntimeStop(asset.id);
      else await transport.serviceRuntimeRestart(asset.id);
    } catch (err) {
      setActionError({ id: asset.id, message: normalizeDesktopError(err).message });
    } finally {
      operationRef.current = false;
      setPending(null);
      void refresh();
    }
  };
  const open = async (asset: AssetSummary) => {
    setActionError(null);
    try { await transport.serviceOpenPage(asset.id); }
    catch (err) { setActionError({ id: asset.id, message: normalizeDesktopError(err).message }); }
  };
  const copy = async (value: string, kind: 'address' | 'logs') => {
    try {
      await navigator.clipboard.writeText(value);
      clearTimeout(copyTimer.current);
      setCopied(kind);
      copyTimer.current = setTimeout(() => setCopied(null), 1500);
    } catch (err) {
      if (selected) setActionError({ id: selected.id, message: normalizeDesktopError(err).message });
    }
  };
  const badge = (state?: ServiceRuntimeState) => {
    const display = state === 'external' ? 'running' : state;
    return <span data-testid={`service-state-${state ?? 'unknown'}`} className={`service-status service-status-${display ?? 'unknown'}`}>
      <span className="service-status-dot" />{display ? t(display) : t(runtimeSupported ? 'Checking…' : 'Not checked')}
    </span>;
  };
  const controls = (asset: AssetSummary, compact = false) => {
    const config = recordOf(asset);
    const state = stateOf(asset);
    const busy = pending?.id === asset.id;
    const transitional = state === 'starting' || state === 'stopping' || busy;
    const disabled = Boolean(pending) || transitional;
    const configured = Boolean(config?.start_command && config?.project_dir && asset.lifecycle === 'active');
    const canStop = state !== 'external' || Boolean(config?.stop_command && config?.project_dir);
    const stopTitle = canStop ? t('Stop') : t('Configure a stop command in Edit Configuration to control this service');
    const workingLabel = busy ? t(pending.operation === 'start' ? 'Starting...' : pending.operation === 'stop' ? 'Stopping...' : 'Restarting…') : null;
    return <div className={`service-controls${compact ? ' service-controls-compact' : ''}`} onClick={(event) => event.stopPropagation()}>
      {config?.endpoint_url && isRunning(state) && <button type="button" data-testid="service-open-page-button"
        className={compact ? 'service-link' : 'service-button service-button-primary'} onClick={() => void open(asset)}>
        {t(compact ? 'Open' : 'Open Page')}<Icon name="externalLink" size={14} />
      </button>}
      {runtimeSupported && state && (isRunning(state) || state === 'starting' || state === 'stopping') && <>
        {!compact && isRunning(state) && configured && <button type="button" data-testid="service-restart-button"
          className="service-button" disabled={disabled || !canStop} title={canStop ? t('Restart') : stopTitle} onClick={() => void run(asset, 'restart')}>
          <Icon name="refresh" size={15} />{busy && pending.operation === 'restart' ? workingLabel : t('Restart')}
        </button>}
        <button type="button" data-testid="service-stop-button" className="service-button"
          disabled={disabled || !canStop} title={stopTitle} onClick={() => void run(asset, 'stop')}>
          {!compact && <Icon name="stop" size={14} />}{busy && pending.operation !== 'restart' ? workingLabel : state === 'stopping' ? t('Stopping...') : t('Stop')}
        </button>
      </>}
      {runtimeSupported && configured && (state === 'stopped' || state === 'failed') && <button type="button"
        data-testid="service-start-button" className="service-button service-button-primary" disabled={disabled} onClick={() => void run(asset, 'start')}>
        <Icon name="play" size={15} />{workingLabel ?? t('Start')}
      </button>}
      {!compact && runtimeSupported && !configured && !isRunning(state) && <button type="button"
        className="service-button" onClick={() => setEditingId(asset.id)}>{t('Configure launch command')}</button>}
    </div>;
  };

  return <main data-testid="services-workspace" className="services-workspace">
    <header className="services-header">
      <div><h2>{t('Services')}</h2><p>{t('Manage access and launch configuration for local services')}</p></div>
      <button type="button" data-testid="new-asset-button" className="service-button service-button-primary" onClick={onNewAsset}>+ {t('Add Service')}</button>
    </header>
    <div className="services-toolbar">
      <div className="service-status-filters" role="group" aria-label={t('Filter by runtime status')}>
        {(['all', 'running', 'stopped'] as const).map((value) => <button key={value} type="button" aria-pressed={filter === value}
          className={filter === value ? 'is-active' : ''} onClick={() => setFilter(value)}>
          {t(value === 'all' ? 'All' : value)}<span>{value === 'all' ? items.length : items.filter((asset) => value === 'running'
            ? isRunning(stateOf(asset)) : ['stopped', 'failed'].includes(stateOf(asset) ?? '')).length}</span>
        </button>)}
      </div>
      <div className="service-search"><Icon name="search" size={16} /><input ref={searchRef} type="search" aria-label={t('Search services')}
        placeholder={t('Search services…')} value={searchQuery} onChange={(event) => onSearchChange(event.target.value)} /></div>
      <button type="button" className="service-button" onClick={() => void refresh()} disabled={!runtimeSupported}><Icon name="refresh" size={16} />{t('Refresh status')}</button>
      {filtered && <button className="service-link" onClick={() => { setFilter('all'); onResetFilters(); }}>{t('Reset filters')}</button>}
    </div>
    {totalPages > 1 && <p className="services-page-note">{t('Status filters and counts apply to the current page')}</p>}
    {error && <div role="alert" className="service-error"><strong>{t('Query Execution Failed')}</strong><p>{t(error.message)}</p><button className="service-button" onClick={onRetry}>{t('Retry')}</button></div>}
    {statusError && <div role="alert" className="service-error">{t('Could not refresh service status. Showing the last known status.')} <button className="service-link" onClick={() => void refresh()}>{t('Retry')}</button></div>}
    <div className="services-content">
      <section className="services-list" aria-label={t('Services')} aria-busy={loading}>
        <div className="service-list-columns" aria-hidden="true"><span>{t('Service')}</span><span>{t('Status')}</span><span>{t('Actions')}</span></div>
        {loading && !data ? <p className="services-empty">{t('Loading assets...')}</p> : visibleItems.map((asset) => {
          const config = recordOf(asset);
          return <div key={asset.id} role="row" tabIndex={0} aria-selected={asset.id === selectedAssetId}
            data-testid="service-row" className={`service-row${asset.id === selectedAssetId ? ' is-selected' : ''}`}
            onClick={() => onSelectAsset(asset.id)} onKeyDown={(event) => {
              if (event.target !== event.currentTarget) return;
              if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); onSelectAsset(asset.id); }
            }}>
            <div className="service-row-name"><strong>{asset.name}</strong><span className={config?.endpoint_url ? 'service-mono' : ''}>{config?.endpoint_url || t('Local Service')}</span>
              {asset.lifecycle !== 'active' && <small>{t(asset.lifecycle)}</small>}</div>
            {badge(stateOf(asset))}{controls(asset, true)}
            {actionError?.id === asset.id && <div role="alert" data-testid="service-action-error" className="service-row-error">{t(actionError.message)}</div>}
          </div>;
        })}
        {!loading && !error && visibleItems.length === 0 && <div className="services-empty">
          <strong>{t(filtered ? 'No matching services' : 'No services yet')}</strong>
          <p>{t(filtered ? 'Try another keyword or reset filters.' : 'Add a local project you start by hand, and run it from here.')}</p>
          <button className="service-button service-button-primary" onClick={filtered ? () => { setFilter('all'); onResetFilters(); } : onNewAsset}>{t(filtered ? 'Reset filters' : 'Add Service')}</button>
        </div>}
        <footer className="service-list-footer"><span>{t('{n} services recorded', { n: total })}</span>
          {totalPages > 1 && <div><button className="service-link" disabled={page === 1} onClick={() => onSelectPage(page - 1)}>{t('Previous')}</button><span>{page} / {totalPages}</span><button className="service-link" disabled={page === totalPages} onClick={() => onSelectPage(page + 1)}>{t('Next')}</button></div>}
        </footer>
      </section>
      <aside className="service-inspector" aria-label={t('Service Details')}>
        {selected ? <>
          <header className="service-inspector-header"><div><span className="service-kind">{t('Local Service')}</span><h3>{selected.name}</h3>{badge(stateOf(selected))}</div>
            <button className="service-button service-more" title={t('Service Details')} aria-label={t('Service Details')} onClick={() => onOpenDetail(selected.id)}><Icon name="more" /></button></header>
          {statuses[selected.id]?.error && <div role="alert" data-testid="service-runtime-error" className="service-error">{t(statuses[selected.id].error!)}</div>}
          <section className="service-access"><h4>{t('Access Address')}</h4><div className="service-address"><code>{record?.endpoint_url || t('Not configured')}</code>
            {record?.endpoint_url && <button className="service-link" aria-label={t('Copy access address')} title={t('Copy access address')} onClick={() => void copy(record.endpoint_url!, 'address')}><Icon name={copied === 'address' ? 'check' : 'copy'} size={17} /></button>}</div>{controls(selected)}</section>
          <section className="service-config"><div className="service-section-heading"><h4>{t('Start and stop')}</h4><button className="service-link" data-testid="service-edit-button" onClick={() => setEditingId(selected.id)}><Icon name="edit" size={15} />{t('Edit Configuration')}</button></div>
            <dl><div><dt>{t('Project Directory')}</dt><dd data-testid="service-detail-dir">{record?.project_dir || '—'}</dd></div>
              <div><dt>{t('Start Command')}</dt><dd data-testid="service-detail-command">{record?.start_command || '—'}</dd></div>
              <div><dt>{t('Stop method')}</dt><dd data-testid="service-detail-stop-command">{record?.stop_command || t(stateOf(selected) === 'external' ? 'Not configured' : 'Stop the bound process tree')}</dd></div></dl>
            {!record?.stop_command && stateOf(selected) !== 'external' && <p className="service-config-hint">{t('Records the process on launch; stopping ends the service and its child processes.')}</p>}
            {stateOf(selected) === 'external' && !record?.stop_command && <button className="service-link service-config-hint" onClick={() => setEditingId(selected.id)}>{t('Configure a stop command to enable Stop and Restart')}</button>}
            {record?.notes && <p className="service-notes">{record.notes}</p>}
            <p className="service-updated">{t('Last Updated')} {formatRelativeTime(selected.updated_at)}</p>
          </section>
          <section className="service-log-section"><div className="service-section-heading"><h4>{t('Runtime logs')}</h4>{logs.length > 0 && <div className="service-log-actions">
            <button className="service-link" onClick={() => setExpandedLogs(!expandedLogs)}>{t(expandedLogs ? 'Collapse' : 'View all')}</button>
            <button className="service-link" data-testid="service-copy-logs-button" onClick={() => void copy(logs.map((line) => `[${line.timestamp}] ${line.stream}: ${line.text}`).join('\n'), 'logs')}>{t(copied === 'logs' ? 'Copied!' : 'Copy logs')}</button></div>}</div>
            <div data-testid="service-logs-panel" className={`service-logs${expandedLogs ? ' is-expanded' : ''}`}>
              {logs.length === 0 ? <span>{t('Output appears here after launching the service.')}</span> : (expandedLogs ? logs : logs.slice(-5)).map((line) => <div key={line.seq} className={line.stream === 'stderr' ? 'service-log-stderr' : ''}>[{logTime(line.timestamp)}] {line.text}</div>)}
            </div>{logsDropped && <p className="service-config-hint">{t('Older output was truncated to keep the log buffer bounded.')}</p>}
          </section>
        </> : <div className="services-empty">{t('Select a service to view details')}</div>}
      </aside>
    </div>
    {editingId && <CreateLocalServiceModal key={editingId} isOpen assetId={editingId}
      onClose={() => setEditingId(null)} onCreated={() => { onRetry(); void refresh(); }} />}
  </main>;
};
