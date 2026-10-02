import React, { useEffect, useRef, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type { DesktopError, ServiceRecordDto } from './types';
import { t } from '../../i18n';

interface CreateLocalServiceModalProps {
  isOpen: boolean;
  assetId?: string;
  onClose: () => void;
  onCreated: (newAssetId: string) => void;
}

const fieldLabel: React.CSSProperties = {
  display: 'block',
  fontSize: '12px',
  fontWeight: 500,
  marginBottom: '4px',
};

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '8px 10px',
  fontSize: '13px',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  boxSizing: 'border-box',
  fontFamily: 'inherit',
};

/**
 * Adds a manually-started local project (服务 page): the launch configuration
 * is saved as typed fields of a `local` service record; nothing is started
 * until the user presses Start.
 */
export const CreateLocalServiceModal: React.FC<CreateLocalServiceModalProps> = ({
  isOpen,
  assetId,
  onClose,
  onCreated,
}) => {
  const dialogRef = useRef<HTMLDivElement>(null);
  const [name, setName] = useState('');
  const [projectDir, setProjectDir] = useState('');
  const [startCommand, setStartCommand] = useState('');
  const [stopCommand, setStopCommand] = useState('');
  const [saved, setSaved] = useState<{ revision: number; record: ServiceRecordDto } | null>(null);
  const [loading, setLoading] = useState(Boolean(assetId));
  const [runtimeBusy, setRuntimeBusy] = useState(false);
  const [endpointUrl, setEndpointUrl] = useState('');
  const [notes, setNotes] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  useEffect(() => {
    if (!isOpen) return;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    dialogRef.current?.querySelector<HTMLElement>('input:not(:disabled), button:not(:disabled)')?.focus();
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Tab') {
        const fields = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), textarea:not(:disabled)') ?? []);
        const first = fields[0]; const last = fields[fields.length - 1];
        if (!dialogRef.current?.contains(document.activeElement)) { e.preventDefault(); first?.focus(); }
        else if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
      }
      if (e.key === 'Escape' && !submitting) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
      previousFocus?.focus();
  }, [isOpen, onClose, submitting]);

  useEffect(() => {
    if (!isOpen || !assetId) return;
    let alive = true;
    setLoading(true);
    const transport = getTransport();
    void Promise.all([transport.getAsset(assetId), transport.serviceRuntimeStatus(assetId)]).then(([detail, status]) => {
      if (!alive) return;
      if (detail.details.module !== 'services') throw new Error('Not a service');
      const record = detail.details;
      setSaved({ revision: detail.revision, record });
      setName(detail.name);
      setProjectDir(record.project_dir ?? '');
      setStartCommand(record.start_command ?? '');
      setStopCommand(record.stop_command ?? '');
      setEndpointUrl(record.endpoint_url ?? '');
      setNotes(record.notes ?? '');
      setRuntimeBusy(['running', 'starting', 'stopping'].includes(status.state));
    }).catch((err) => { if (alive) setError(normalizeDesktopError(err)); })
      .finally(() => { if (alive) setLoading(false); });
    return () => { alive = false; };
  }, [assetId, isOpen]);

  if (!isOpen) return null;

  const transport = getTransport();

  const chooseDirectory = async () => {
    setPicking(true);
    try {
      const dir = await transport.pickDirectory(t('Choose the project directory'));
      if (dir) setProjectDir(dir);
    } catch {
      // The chooser failing keeps the typed value; it can also be typed by hand.
    } finally {
      setPicking(false);
    }
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (submitting || loading || (assetId && !saved)) return;

    if (!name.trim()) {
      setError({ category: 'invalid_input', message: 'Service name must not be empty' });
      return;
    }
    if (!projectDir.trim()) {
      setError({ category: 'invalid_input', message: 'Project directory must not be empty' });
      return;
    }
    if (!startCommand.trim()) {
      setError({ category: 'invalid_input', message: 'Start command must not be empty' });
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const patch = (value: string, previous?: string | null) => value.trim() === (previous ?? '') ? undefined : value.trim();
      const receipt = await transport.serviceCommand(assetId && saved ? {
        action: 'update', asset_id: assetId, expected_revision: saved.revision,
        name: name.trim(), notes: patch(notes, saved.record.notes),
        endpoint_url: patch(endpointUrl, saved.record.endpoint_url),
        project_dir: patch(projectDir, saved.record.project_dir),
        start_command: patch(startCommand, saved.record.start_command),
        stop_command: patch(stopCommand, saved.record.stop_command),
      } : {
        action: 'create',
        name: name.trim(),
        service_type: 'local',
        endpoint_url: endpointUrl.trim() || undefined,
        notes: notes.trim() || undefined,
        project_dir: projectDir.trim(),
        start_command: startCommand.trim(),
        stop_command: stopCommand.trim() || undefined,
      });

      const newId = assetId ?? receipt.asset_ids[0];
      onCreated(newId);
      onClose();
    } catch (err: unknown) {
      setError(normalizeDesktopError(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-label={t(assetId ? 'Edit Configuration' : 'Add Local Service')}
      style={{
        position: 'fixed',
        inset: 0,
        backgroundColor: 'rgba(0, 0, 0, 0.45)',
        backdropFilter: 'blur(2px)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        zIndex: 110,
        padding: '24px',
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget && !submitting) {
          onClose();
        }
      }}
    >
      <div
        style={{
          width: '100%',
          maxWidth: '520px',
          maxHeight: '88vh',
          backgroundColor: 'var(--color-surface)',
          borderRadius: 'var(--radius-lg)',
          border: '1px solid var(--color-border)',
          boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            padding: '16px 20px',
            borderBottom: '1px solid var(--color-border)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Badge variant="mesh">{t('Service')}</Badge>
            <h3 style={{ margin: 0, fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)' }}>
              {t(assetId ? 'Edit Configuration' : 'Add Local Service')}
            </h3>
          </div>
          <button
            onClick={onClose}
            disabled={submitting || loading}
            aria-label={t(assetId ? 'Close configuration' : 'Close add service modal')}
            style={{
              background: 'none',
              border: 'none',
              cursor: submitting ? 'not-allowed' : 'pointer',
              fontSize: '16px',
              color: 'var(--color-muted)',
            }}
          >
            ✕
          </button>
        </div>

        <form
          onSubmit={handleSubmit}
          data-testid="create-local-service-form"
          style={{
            padding: '20px',
            overflowY: 'auto',
            display: 'flex',
            flexDirection: 'column',
            gap: '14px',
          }}
        >
          {error && (
            <div
              role="alert"
              data-testid="create-local-service-error"
              style={{
                padding: '10px 14px',
                backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
                border: '1px solid var(--color-danger)',
                borderRadius: 'var(--radius-md)',
                color: 'var(--color-danger)',
                fontSize: '12px',
              }}
            >
              <div style={{ fontWeight: 600, marginBottom: '2px' }}>{t(error.category)}</div>
              <div>{t(error.message)}</div>
            </div>
          )}

          <div>
            <label htmlFor="create-local-name" style={fieldLabel}>{t('Service Name *')}</label>
            <input
              id="create-local-name"
              data-testid="local-service-create-name-input"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={submitting || loading}
              placeholder={t('e.g. gcli2api, dev proxy')}
              style={inputStyle}
            />
          </div>

          <div>
            <label htmlFor="create-local-dir" style={fieldLabel}>{t('Project Directory *')}</label>
            <div style={{ display: 'flex', gap: 8 }}>
              <input
                id="create-local-dir"
                data-testid="local-service-create-dir-input"
                type="text"
                required
                value={projectDir}
                onChange={(e) => setProjectDir(e.target.value)}
                disabled={submitting || loading || runtimeBusy}
                placeholder="/Users/me/projects/my-service"
                style={inputStyle}
              />
              <button
                type="button"
                data-testid="local-service-choose-dir-button"
                onClick={chooseDirectory}
                disabled={submitting || loading || picking || runtimeBusy}
                style={{
                  padding: '8px 12px',
                  backgroundColor: 'var(--color-canvas)',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '12px',
                  cursor: submitting || picking ? 'wait' : 'pointer',
                  whiteSpace: 'nowrap',
                }}
              >
                {t('Choose…')}
              </button>
            </div>
          </div>

          <div>
            <label htmlFor="create-local-command" style={fieldLabel}>{t('Start Command *')}</label>
            <input
              id="create-local-command"
              data-testid="local-service-create-command-input"
              type="text"
              required
              value={startCommand}
              onChange={(e) => setStartCommand(e.target.value)}
              disabled={submitting || loading || runtimeBusy}
              placeholder={t('e.g. bash start-local.sh')}
              style={{ ...inputStyle, fontFamily: 'var(--font-mono)' }}
            />
            <div style={{ fontSize: '11px', color: 'var(--color-muted)', marginTop: 4 }}>
              {t('Runs in the project directory when you press Start. Nothing starts automatically.')}
              <br />{t('Keep credentials in the project environment file rather than in saved commands. Review imported commands before running them.')}
            </div>
          </div>

          {runtimeBusy && <p style={{ fontSize: 12, color: 'var(--color-muted)' }}>{t('Stop the service before changing its launch commands.')}</p>}
          <div>
            <label htmlFor="create-local-stop-command" style={fieldLabel}>{t('Stop Command (optional)')}</label>
            <input id="create-local-stop-command" data-testid="local-service-stop-command-input" type="text"
              value={stopCommand} onChange={(event) => setStopCommand(event.target.value)}
              disabled={submitting || loading || runtimeBusy} placeholder={t('e.g. bash stop-local.sh')}
              style={{ ...inputStyle, fontFamily: 'var(--font-mono)' }} />
            <p style={{ fontSize: 11, color: 'var(--color-muted)', marginTop: 4 }}>
              {t('Leave empty to stop the bound process tree. Set a stop command to control an existing service.')}
            </p>
          </div>

          <div>
            <label htmlFor="create-local-url" style={fieldLabel}>{t('Access Address (optional)')}</label>
            <input
              id="create-local-url"
              data-testid="local-service-create-url-input"
              type="text"
              value={endpointUrl}
              onChange={(e) => setEndpointUrl(e.target.value)}
              disabled={submitting || loading}
              placeholder="http://127.0.0.1:7861"
              style={inputStyle}
            />
            <div style={{ fontSize: '11px', color: 'var(--color-muted)', marginTop: 4 }}>
              {t('When the service runs, this is where its page opens.')}
            </div>
          </div>

          <div>
            <label htmlFor="create-local-notes" style={fieldLabel}>{t('Notes (optional)')}</label>
            <textarea
              id="create-local-notes"
              data-testid="local-service-create-notes-input"
              rows={3}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              disabled={submitting || loading}
              style={inputStyle}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '12px' }}>
            <button
              type="button"
              onClick={onClose}
              disabled={submitting || loading}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >{t('Cancel')}</button>
            <button
              type="submit"
              data-testid="submit-create-local-service-button"
              disabled={submitting || loading || Boolean(assetId && !saved)}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-mesh)',
                color: '#fff',
                border: 'none',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                fontWeight: 500,
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >
              {submitting ? t(assetId ? 'Saving...' : 'Creating...') : t('Save Service')}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
