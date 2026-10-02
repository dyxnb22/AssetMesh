import { useCallback, useEffect, useState } from 'react';
import { localeTag, setLang, t, useLang } from '../i18n';
import { useBackupPreferenceSync } from '../features/library/useBackupPreferenceSync';
import { readBackupPreferenceValues, restoreUnchangedBackupPreferences } from '../features/library/backup-preferences';
import type { DesktopTransport } from '../features/library/transport';
import type { AppCapabilities, AppStatus, ThemePreference } from '../features/library/types';

/** Owns startup/recovery and preference persistence for one desktop session. */
export function useDesktopSession(transport: DesktopTransport) {
  const lang = useLang();
  const [status, setStatus] = useState<AppStatus>({ status: 'loading' });
  const [capabilities, setCapabilities] = useState<AppCapabilities | null>(null);
  const [restorePending, setRestorePending] = useState(false);
  const [preferencesReady, setPreferencesReady] = useState(false);
  useEffect(() => {
    const pending = () => setRestorePending(true);
    window.addEventListener('assetmesh-restore-pending', pending);
    return () => window.removeEventListener('assetmesh-restore-pending', pending);
  }, []);

  const [theme, setTheme] = useState<ThemePreference>(() => {
    try {
      const saved = localStorage.getItem('assetmesh-theme');
      if (saved === 'light' || saved === 'dark' || saved === 'system') {
        return saved;
      }
    } catch {
      // Fallback if localStorage unavailable
    }
    return 'system';
  });

  const handleThemeChange = (newTheme: ThemePreference) => {
    setTheme(newTheme);
    try {
      localStorage.setItem('assetmesh-theme', newTheme);
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    const root = document.documentElement;
    root.lang = localeTag();
    if (theme !== 'system') {
      root.dataset.theme = theme;
      return;
    }
    const appearance = window.matchMedia?.('(prefers-color-scheme: dark)');
    const applyAppearance = () => { root.dataset.theme = appearance?.matches ? 'dark' : 'light'; };
    applyAppearance();
    appearance?.addEventListener('change', applyAppearance);
    return () => appearance?.removeEventListener('change', applyAppearance);
  }, [theme, lang]);

  const restorePreferences = useCallback(async (disposed: () => boolean = () => false) => {
    setPreferencesReady(false);
    const previous = readBackupPreferenceValues();
    try {
      const preferences = await transport.backupPreferences();
      if (disposed()) return;
      const restored = restoreUnchangedBackupPreferences(preferences, previous);
      const savedTheme = restored['assetmesh-theme'];
      if (savedTheme === 'system' || savedTheme === 'light' || savedTheme === 'dark') setTheme(savedTheme);
      const savedLang = restored['assetmesh-lang'];
      if (savedLang === 'zh' || savedLang === 'en') setLang(savedLang);
    } catch { /* Backup errors are displayed in Settings; the library remains usable. */ }
    finally { if (!disposed()) setPreferencesReady(true); }
  }, [transport]);

  useEffect(() => {
    let disposed = false;
    const load = async () => {
      try {
        const appStatus = await transport.getStatus();
        if (disposed) return;
        // Canonical reads can begin without waiting for preference filesystem I/O.
        setStatus(appStatus);
        if (appStatus.status === 'ready') {
          void restorePreferences(() => disposed);
          const caps = await transport.getCapabilities();
          if (!disposed) setCapabilities(caps);
        }
      } catch (err: unknown) {
        if (!disposed) setStatus({
          status: 'setup_failure',
          message: err instanceof Error ? err.message : String(err),
        });
      }
    };
    void load();
    return () => { disposed = true; };
  }, [transport, restorePreferences]);

  useBackupPreferenceSync(transport, status.status === 'ready' && preferencesReady, theme, lang);

  const [setupBusy, setSetupBusy] = useState(false);

  // Actually re-opens the database: `getStatus` alone can never leave the failure
  // state, because the backend only retries when it is asked to initialize.
  const retrySetup = useCallback(
    async (dbPath?: string) => {
      setSetupBusy(true);
      try {
        const next = await transport.init(dbPath);
        setStatus(next);
        if (next.status === 'ready') {
          void restorePreferences();
          setCapabilities(await transport.getCapabilities());
        }
      } catch (err: unknown) {
        setStatus({
          status: 'setup_failure',
          message: err instanceof Error ? err.message : String(err),
        });
      } finally {
        setSetupBusy(false);
      }
    },
    [transport, restorePreferences],
  );

  const chooseDatabaseFolder = useCallback(async () => {
    const dir = await transport.pickDirectory(t('Choose a folder for the AssetMesh database'));
    if (dir) await retrySetup(`${dir}/assetmesh.db`);
  }, [retrySetup, t, transport]);

  return { status, capabilities, restorePending, setupBusy, retrySetup, chooseDatabaseFolder, theme, handleThemeChange };
}
