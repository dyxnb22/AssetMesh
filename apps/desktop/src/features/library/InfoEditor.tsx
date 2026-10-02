import React, { useEffect, useState } from 'react';
import { t } from '../../i18n';
import { getTransport, normalizeDesktopError } from './transport';
import type { AssetDetailDto, InfoRecordDto, InfoType, MutationReceiptDto } from './types';

const kinds: InfoType[] = ['email', 'url', 'api_key', 'text'];
const kindLabels: Record<InfoType, string> = {
  email: 'Email', url: 'URL', api_key: 'API key', text: 'Text',
};

const fieldStyle: React.CSSProperties = {
  width: '100%', padding: '8px 10px', border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)', backgroundColor: 'var(--color-canvas)',
  color: 'var(--color-ink)', boxSizing: 'border-box', font: 'inherit',
};

interface EditorProps {
  detail?: AssetDetailDto;
  onSaved: (receipt: MutationReceiptDto, fresh?: AssetDetailDto) => void;
  onCancel: () => void;
}

export const InfoEditor: React.FC<EditorProps> = ({ detail, onSaved, onCancel }) => {
  const record = detail?.details.module === 'info' ? detail.details : null;
  const [name, setName] = useState(detail?.name ?? '');
  const [infoType, setInfoType] = useState<InfoType>(record?.info_type ?? 'text');
  const [value, setValue] = useState(record?.value ?? '');
  const [notes, setNotes] = useState(record?.notes ?? '');
  const [tags, setTags] = useState(detail?.tags.join(', ') ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!name.trim() || !value.trim()) {
      setError(t('Name and value are required.'));
      return;
    }
    setBusy(true);
    setError('');
    try {
      const transport = getTransport();
      const receipt = detail
        ? await transport.infoCommand({
            action: 'update', asset_id: detail.id, expected_revision: detail.revision,
            name: name.trim(), info_type: infoType, value: value.trim(), notes: notes.trim() || null,
            tags: tags.split(',').map((tag) => tag.trim()).filter(Boolean),
          })
        : await transport.infoCommand({
            action: 'create', name: name.trim(), info_type: infoType, value: value.trim(),
            notes: notes.trim() || null,
            tags: tags.split(',').map((tag) => tag.trim()).filter(Boolean),
          });
      const fresh = detail ? await transport.getAsset(detail.id) : undefined;
      onSaved(receipt, fresh);
    } catch (reason) {
      setError(normalizeDesktopError(reason).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} style={{ display: 'grid', gap: '12px' }}>
      <label>{t('Name')}
        <input data-testid="info-name-input" style={fieldStyle} value={name} onChange={(e) => setName(e.target.value)} maxLength={512} autoFocus />
      </label>
      <label>{t('Information type')}
        <select data-testid="info-type-input" style={fieldStyle} value={infoType} onChange={(e) => setInfoType(e.target.value as InfoType)}>
          {kinds.map((kind) => <option key={kind} value={kind}>{t(kindLabels[kind])}</option>)}
        </select>
      </label>
      <label>{t('Value')}
        <textarea data-testid="info-value-input" style={{ ...fieldStyle, minHeight: '88px', resize: 'vertical' }} value={value} onChange={(e) => setValue(e.target.value)} maxLength={16384} />
      </label>
      {infoType === 'api_key' && <p style={{ margin: '0 0 8px', fontSize: '12px', color: 'var(--color-muted)' }}>{t('API keys are stored without encryption in this library and its recovery backups. Use encrypted storage for sensitive keys.')}</p>}
      <label>{t('Notes')}
        <textarea data-testid="info-notes-input" style={{ ...fieldStyle, minHeight: '64px', resize: 'vertical' }} value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={8192} />
      </label>
      <label>{t('Tags (comma separated)')}
        <input data-testid="info-tags-input" style={fieldStyle} value={tags} onChange={(e) => setTags(e.target.value)} />
      </label>
      {error && <div role="alert" style={{ color: 'var(--color-danger)', fontSize: '12px' }}>{error}</div>}
      <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px' }}>
        <button type="button" onClick={onCancel} disabled={busy}>{t('Cancel')}</button>
        <button type="submit" disabled={busy} data-testid="info-save-button">{busy ? t('Saving...') : t('Save')}</button>
      </div>
    </form>
  );
};

export const CreateInfoModal: React.FC<{ onClose: () => void; onCreated: (id: string) => void }> = ({ onClose, onCreated }) => (
  <div role="dialog" aria-modal="true" aria-label={t('New Information')} style={{
    position: 'fixed', inset: 0, zIndex: 120, backgroundColor: 'rgba(0,0,0,.45)',
    display: 'grid', placeItems: 'center', padding: '20px',
  }} onClick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <div style={{ width: 'min(520px, 100%)', maxHeight: '90vh', overflowY: 'auto', padding: '24px',
      backgroundColor: 'var(--color-surface)', borderRadius: 'var(--radius-lg)' }}>
      <h2 style={{ margin: '0 0 16px', fontSize: '18px' }}>{t('New Information')}</h2>
      <InfoEditor onCancel={onClose} onSaved={(receipt) => onCreated(receipt.asset_ids[0])} />
    </div>
  </div>
);

export const InfoPanel: React.FC<{
  detail: AssetDetailDto;
  record: InfoRecordDto;
  onSaved: (receipt: MutationReceiptDto, fresh: AssetDetailDto) => void;
}> = ({ detail, record, onSaved }) => {
  const [editing, setEditing] = useState(false);
  const [copied, setCopied] = useState(false);
  const [revealed, setRevealed] = useState(false);
  useEffect(() => { setRevealed(false); setCopied(false); }, [detail.id, record.value]);
  const [error, setError] = useState('');
  if (editing) return <InfoEditor detail={detail} onCancel={() => setEditing(false)} onSaved={(receipt, fresh) => {
    if (fresh) onSaved(receipt, fresh);
    setEditing(false);
  }} />;
  return (
    <section style={{ padding: '14px', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-md)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: '12px', alignItems: 'center' }}>
        <strong>{t(kindLabels[record.info_type])}</strong>
        <div style={{ display: 'flex', gap: '8px' }}>
          {record.info_type === 'api_key' && <button type="button" onClick={() => setRevealed(!revealed)}>{revealed ? t('Hide value') : t('Show value')}</button>}
          <button type="button" data-testid="info-copy-button" onClick={async () => {
            try { await navigator.clipboard.writeText(record.value); setCopied(true); setError(''); }
            catch { setError(t('Could not copy value.')); }
          }}>{copied ? t('Copied') : t('Copy value')}</button>
          {detail.lifecycle === 'active' && <button type="button" onClick={() => setEditing(true)}>{t('Edit')}</button>}
        </div>
      </div>
      <pre data-testid="info-value" style={{ whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', font: 'inherit', margin: '12px 0' }}>{record.info_type === 'api_key' && !revealed ? '••••••••••••' : record.value}</pre>
      {record.notes && <p style={{ color: 'var(--color-muted)', whiteSpace: 'pre-wrap' }}>{record.notes}</p>}
      {error && <p role="alert" style={{ color: 'var(--color-danger)' }}>{error}</p>}
    </section>
  );
};
