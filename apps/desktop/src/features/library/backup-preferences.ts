const keys = ['assetmesh-saved-filters-v1', 'assetmesh-theme', 'assetmesh-lang'] as const;

export function readBackupPreferenceValues(): Record<string, string | null> {
  const values: Record<string, string | null> = {};
  for (const key of keys) {
    try { values[key] = localStorage.getItem(key); }
    catch { values[key] = null; }
  }
  return values;
}

/** A late recovery read must not overwrite a choice made since it started. */
export function restoreUnchangedBackupPreferences(
  preferences: Record<string, string>,
  previous: Record<string, string | null>,
): Record<string, string> {
  const current = readBackupPreferenceValues();
  const unchanged: Record<string, string> = {};
  for (const key of keys) {
    if (key in preferences && current[key] === previous[key]) unchanged[key] = preferences[key];
  }
  applyBackupPreferences(unchanged);
  return unchanged;
}

export function captureBackupPreferences(): Record<string, string> {
  const preferences: Record<string, string> = { 'assetmesh-saved-filters-v1': '[]', 'assetmesh-theme': 'system', 'assetmesh-lang': 'zh' };
  for (const key of keys) {
    const value = localStorage.getItem(key);
    if (value !== null) preferences[key] = value;
  }
  return preferences;
}

export function applyBackupPreferences(preferences: Record<string, string>): void {
  for (const key of keys) {
    if (key in preferences) localStorage.setItem(key, preferences[key]);
  }
}
