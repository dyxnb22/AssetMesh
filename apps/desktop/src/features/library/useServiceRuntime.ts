import { useCallback, useEffect, useRef, useState } from 'react';
import { getTransport } from './transport';
import type { ServiceRuntimeLogLineDto, ServiceRuntimeStatusDto } from './types';

const ACTIVE_POLL_MS = 5000;
const TRANSITION_POLL_MS = 1500;
const IDLE_POLL_MS = 15000;
const STATUS_FIELDS = ['state', 'pid', 'started_at', 'exit_code', 'exit_signal', 'error'] as const;

function sameStatuses(previous: Record<string, ServiceRuntimeStatusDto>, next: Record<string, ServiceRuntimeStatusDto>) {
  const ids = Object.keys(next);
  return ids.length === Object.keys(previous).length && ids.every((id) => previous[id]
    && STATUS_FIELDS.every((field) => previous[id][field] === next[id][field]));
}

/** Visible-only, non-overlapping polling; action refreshes also pick up the log tail. */
export function useServiceRuntime(selectedAssetId: string | null, supported: boolean) {
  const transport = getTransport();
  const [statusError, setStatusError] = useState(false);
  const statusErrorRef = useRef(false);
  const [statuses, setStatuses] = useState<Record<string, ServiceRuntimeStatusDto>>({});
  const [logState, setLogState] = useState<{ assetId: string | null; lines: ServiceRuntimeLogLineDto[]; dropped: boolean }>({ assetId: null, lines: [], dropped: false });
  const refreshRef = useRef<() => Promise<void>>(async () => {});
  const refresh = useCallback(() => refreshRef.current(), []);

  useEffect(() => {
    if (!supported) { refreshRef.current = async () => {}; return; }
    let alive = true;
    let pending = false;
    let queued = false;
    let queuedForce = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let since = 0;
    let runId: string | null = null;
    let dropped: boolean | undefined;
    let previousStatuses: Record<string, ServiceRuntimeStatusDto> | null = null;
    const visible = () => document.visibilityState !== 'hidden';
    const poll = async (force = false) => {
      if (!alive || !visible()) return;
      if (pending) { queued = true; queuedForce ||= force; return; }
      clearTimeout(timer);
      pending = true;
      const [rows, tail] = await Promise.all([
        transport.serviceRuntimeStatuses(force).catch(() => null),
        selectedAssetId ? transport.serviceRuntimeLogs(selectedAssetId, since, runId ?? undefined).catch(() => null) : null,
      ]);
      if (!alive) return;
      if (statusErrorRef.current !== !rows) { statusErrorRef.current = !rows; setStatusError(!rows); }
      if (rows) {
        const next: Record<string, ServiceRuntimeStatusDto> = {};
        for (const row of rows) next[row.asset_id] = row;
        // Avoid even scheduling React work when nothing changed.
        if (!previousStatuses || !sameStatuses(previousStatuses, next)) {
          previousStatuses = next;
          setStatuses((previous) => sameStatuses(previous, next) ? previous : next);
        }
      }
      if (tail) {
        const changedRun = tail.run_id !== runId;
        runId = tail.run_id;
        if (changedRun) since = 0;
        if (tail.lines.length) since = tail.lines[tail.lines.length - 1].seq;
        if (changedRun || tail.lines.length || dropped !== tail.dropped) {
          dropped = tail.dropped;
          setLogState((previous) => {
            const sameAsset = previous.assetId === selectedAssetId;
            return { assetId: selectedAssetId, dropped: tail.dropped,
              lines: [...(sameAsset && !changedRun ? previous.lines : []), ...tail.lines].slice(-1000) };
          });
        }
      }
      pending = false;
      if (queued) { const forceNext = queuedForce; queued = false; queuedForce = false; void poll(forceNext); return; }
      const active = !previousStatuses || Object.values(previousStatuses).some((row) => ['starting', 'running', 'stopping', 'external'].includes(row.state));
      const transitioning = rows?.some((row) => row.state === 'starting' || row.state === 'stopping');
      if (visible()) timer = setTimeout(() => void poll(), transitioning ? TRANSITION_POLL_MS : active ? ACTIVE_POLL_MS : IDLE_POLL_MS);
    };
    const visibilityChanged = () => {
      clearTimeout(timer);
      if (visible()) void poll();
    };
    refreshRef.current = () => poll(true);
    document.addEventListener('visibilitychange', visibilityChanged);
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
      refreshRef.current = async () => {};
      document.removeEventListener('visibilitychange', visibilityChanged);
    };
  }, [transport, selectedAssetId, supported]);

  return { statuses: supported ? statuses : {}, refresh, statusError: supported && statusError,
    logs: supported && logState.assetId === selectedAssetId ? logState.lines : [],
    logsDropped: supported && logState.assetId === selectedAssetId && logState.dropped };
}
