import { useEffect, useRef } from 'react';
import type { DesktopTransport } from './transport';
import { captureBackupPreferences } from './backup-preferences';

/** Persist on changes, retry failures three times, and coalesce changes during a save. */
export function useBackupPreferenceSync(transport: DesktopTransport, ready: boolean, theme: string, lang: string) {
  const lastSaved = useRef('');
  const schedule = useRef<() => void>(() => {});
  useEffect(() => {
    if (!ready) return;
    let disposed = false;
    let syncing = false;
    let dirty = false;
    let retries = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const sync = async () => {
      if (disposed) return;
      if (syncing) { dirty = true; return; }
      syncing = true;
      dirty = false;
      const preferences = captureBackupPreferences();
      const serialized = JSON.stringify(preferences);
      try {
        if (serialized !== lastSaved.current) {
          await transport.backupSavePreferences(preferences);
          if (disposed) return;
          lastSaved.current = serialized;
        }
        retries = 0;
      } catch {
        if (!disposed && retries < 3) {
          timer = setTimeout(() => void sync(), [1000, 3000, 10000][retries++]);
        }
      } finally {
        syncing = false;
        if (dirty && !disposed) changed();
      }
    };
    const changed = () => {
      clearTimeout(timer);
      retries = 0;
      timer = setTimeout(() => void sync(), 250);
    };
    const flush = () => {
      const preferences = captureBackupPreferences();
      if (JSON.stringify(preferences) !== lastSaved.current) {
        void transport.backupSavePreferences(preferences).catch(() => {});
      }
    };
    schedule.current = changed;
    changed();
    window.addEventListener('assetmesh-preferences-changed', changed);
    window.addEventListener('pagehide', flush);
    return () => {
      disposed = true;
      clearTimeout(timer);
      schedule.current = () => {};
      window.removeEventListener('assetmesh-preferences-changed', changed);
      window.removeEventListener('pagehide', flush);
      flush();
    };
  }, [transport, ready]);
  useEffect(() => { schedule.current(); }, [theme, lang]);
}
