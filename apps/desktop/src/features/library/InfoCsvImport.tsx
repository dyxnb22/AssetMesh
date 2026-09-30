import React, { useMemo, useState } from 'react';
import { t } from '../../i18n';
import { getTransport, normalizeDesktopError } from './transport';
import type { InfoType } from './types';

type ImportItem = {
  name: string;
  info_type: InfoType;
  value: string;
  notes: string | null;
  tags: string[];
};

function parseRows(input: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let quoted = false;
  let closed = false;
  for (let index = 0; index < input.length; index += 1) {
    const char = input[index];
    if (quoted) {
      if (char === '"' && input[index + 1] === '"') {
        field += '"';
        index += 1;
      } else if (char === '"') {
        quoted = false;
        closed = true;
      } else {
        field += char;
      }
    } else if (char === '"') {
      if (field || closed) throw new Error(t('Invalid CSV quoting.'));
      quoted = true;
    } else if (char === ',') {
      row.push(field);
      field = '';
      closed = false;
    } else if (char === '\n' || char === '\r') {
      if (char === '\r' && input[index + 1] === '\n') index += 1;
      row.push(field);
      if (row.some((part) => part.trim())) rows.push(row);
      row = [];
      field = '';
      closed = false;
    } else {
      if (closed && char !== ' ' && char !== '\t') throw new Error(t('Invalid CSV quoting.'));
      field += char;
    }
  }
  if (quoted) throw new Error(t('Unclosed CSV quote.'));
  row.push(field);
  if (row.some((part) => part.trim())) rows.push(row);
  return rows;
}

function parseInfoCsv(input: string): ImportItem[] {
  const rows = parseRows(input.replace(/^\uFEFF/, ''));
  if (rows.length < 2) throw new Error(t('CSV must contain a header and at least one item.'));
  const header = rows[0].map((name) => name.trim().toLowerCase());
  const column = (name: string) => header.indexOf(name);
  const typeColumn = column('info_type') >= 0 ? column('info_type') : column('type');
  if (column('name') < 0 || typeColumn < 0 || column('value') < 0) {
    throw new Error(t('CSV columns must include name, type, value.'));
  }
  if (rows.length - 1 > 500) throw new Error(t('Import at most 500 items at a time.'));
  const kinds = new Set(['email', 'url', 'api_key', 'text']);
  return rows.slice(1).map((row, index) => {
    if (row.length > header.length) throw new Error(t('CSV row {n} has too many columns.', { n: index + 2 }));
    const name = row[column('name')]?.trim() ?? '';
    const infoType = row[typeColumn]?.trim().toLowerCase() ?? '';
    const value = row[column('value')]?.trim() ?? '';
    if (!name || !value || !kinds.has(infoType)) {
      throw new Error(t('CSV row {n} needs a name, supported type, and value.', { n: index + 2 }));
    }
    const notes = column('notes') >= 0 ? row[column('notes')]?.trim() || null : null;
    const tags = column('tags') >= 0
      ? (row[column('tags')] ?? '').split('|').map((tag) => tag.trim()).filter(Boolean)
      : [];
    return { name, info_type: infoType as InfoType, value, notes, tags };
  });
}

export const InfoCsvImport: React.FC<{ onClose: () => void; onImported: () => void }> = ({ onClose, onImported }) => {
  const [csv, setCsv] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [previewed, setPreviewed] = useState<string | null>(null);
  const parsed = useMemo(() => {
    if (previewed !== csv) return null;
    try { return parseInfoCsv(csv); }
    catch { return null; }
  }, [csv, previewed]);

  const preview = () => {
    try {
      parseInfoCsv(csv);
      setPreviewed(csv);
      setError('');
    } catch (reason) {
      setPreviewed(null);
      setError(reason instanceof Error ? reason.message : t('Invalid CSV.'));
    }
  };

  const apply = async () => {
    if (!parsed) return;
    setBusy(true);
    setError('');
    try {
      await getTransport().infoCommand({ action: 'batch_create', items: parsed });
      onImported();
    } catch (reason) {
      setError(normalizeDesktopError(reason).message);
    } finally {
      setBusy(false);
    }
  };

  return <div role="dialog" aria-modal="true" aria-label={t('Import Information CSV')}
    style={{ position: 'fixed', inset: 0, zIndex: 120, background: 'rgba(0,0,0,.45)', display: 'grid', placeItems: 'center', padding: 20 }}
    onClick={(event) => { if (event.target === event.currentTarget && !busy) onClose(); }}>
    <div style={{ width: 'min(680px, 100%)', maxHeight: '90vh', overflowY: 'auto', padding: 24, background: 'var(--color-surface)', borderRadius: 'var(--radius-lg)' }}>
      <h2 style={{ fontSize: 18, margin: '0 0 12px' }}>{t('Import Information CSV')}</h2>
      <p style={{ color: 'var(--color-muted)', fontSize: 13 }}>{t('Paste CSV with name,type,value,notes,tags columns. Separate multiple tags with |. Up to 500 rows.')}</p>
      <label style={{ display: 'block', marginBottom: 10, fontSize: 13 }}>{t('Choose CSV file')}{' '}
        <input type="file" accept=".csv,text/csv" disabled={busy} onChange={async (event) => {
          const file = event.target.files?.[0];
          if (!file) return;
          try {
            setCsv(await file.text());
            setPreviewed(null);
            setError('');
          } catch {
            setError(t('Could not read CSV file.'));
          }
        }} />
      </label>
      <textarea aria-label={t('Information CSV')} value={csv} onChange={(event) => setCsv(event.target.value)}
        placeholder={'name,type,value,notes,tags\nWork email,email,me@example.com,Primary,work|account'}
        style={{ width: '100%', minHeight: 180, boxSizing: 'border-box', padding: 10, resize: 'vertical', font: '12px monospace', background: 'var(--color-canvas)', color: 'var(--color-ink)', border: '1px solid var(--color-border)' }} />
      {error && <p role="alert" style={{ color: 'var(--color-danger)' }}>{error}</p>}
      {parsed && <div style={{ marginTop: 12, padding: 12, border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)' }}>
        <strong>{t('{n} items ready to import', { n: parsed.length })}</strong>
        <ul style={{ maxHeight: 160, overflowY: 'auto' }}>{parsed.slice(0, 10).map((item, index) => <li key={index}>{item.name} · {item.info_type} · <code>{item.value}</code></li>)}</ul>
        {parsed.length > 10 && <small>{t('Showing first 10 items.')}</small>}
      </div>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>
        <button type="button" onClick={onClose} disabled={busy}>{t('Cancel')}</button>
        <button type="button" onClick={preview} disabled={busy || !csv.trim()}>{t('Preview')}</button>
        <button type="button" onClick={apply} disabled={busy || !parsed}>{busy ? t('Importing...') : t('Import {n} items', { n: parsed?.length ?? 0 })}</button>
      </div>
    </div>
  </div>;
};
