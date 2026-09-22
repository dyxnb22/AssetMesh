import React, { useState } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type { DesktopError } from './types';

interface CreateSoftwareModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated: (newAssetId: string) => void;
}

export const CreateSoftwareModal: React.FC<CreateSoftwareModalProps> = ({
  isOpen,
  onClose,
  onCreated,
}) => {
  const [name, setName] = useState('');
  const [category, setCategory] = useState('application');
  const [installSource, setInstallSource] = useState('homebrew_cask');
  const [version, setVersion] = useState('');
  const [summary, setSummary] = useState('');
  const [installLocation, setInstallLocation] = useState('');
  const [executablePath, setExecutablePath] = useState('');
  const [purpose, setPurpose] = useState('');
  const [notes, setNotes] = useState('');
  const [architecture, setArchitecture] = useState('');
  const [tagsInput, setTagsInput] = useState('');

  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);

  if (!isOpen) return null;

  const transport = getTransport();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (submitting) return;

    if (!name.trim()) {
      setError({
        category: 'invalid_input',
        message: 'Software name must not be empty',
      });
      return;
    }

    setSubmitting(true);
    setError(null);

    const tags = tagsInput
      .split(',')
      .map((t) => t.trim().toLowerCase())
      .filter((t) => t.length > 0);

    try {
      const receipt = await transport.softwareCommand({
        action: 'create',
        name: name.trim(),
        category,
        install_source: installSource || undefined,
        summary: summary.trim() || undefined,
        version: version.trim() || undefined,
        install_location: installLocation.trim() || undefined,
        executable_path: executablePath.trim() || undefined,
        purpose: purpose.trim() || undefined,
        notes: notes.trim() || undefined,
        architecture: architecture.trim() || undefined,
        tags,
      });

      const newId = receipt.asset_ids[0];
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
      role="dialog"
      aria-modal="true"
      aria-label="Add Software Asset"
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
          maxWidth: '560px',
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
            <Badge variant="mesh">Software</Badge>
            <h3 style={{ margin: 0, fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)' }}>
              Add Software Asset
            </h3>
          </div>
          <button
            onClick={onClose}
            disabled={submitting}
            aria-label="Close add software modal"
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
          data-testid="create-software-form"
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
              data-testid="create-software-error"
              style={{
                padding: '10px 14px',
                backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
                border: '1px solid var(--color-danger)',
                borderRadius: 'var(--radius-md)',
                color: 'var(--color-danger)',
                fontSize: '12px',
              }}
            >
              <div style={{ fontWeight: 600, marginBottom: '2px' }}>{error.category}</div>
              <div>{error.message}</div>
            </div>
          )}

          <div>
            <label
              htmlFor="create-software-name"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Name *
            </label>
            <input
              id="create-software-name"
              data-testid="software-create-name-input"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={submitting}
              placeholder="e.g. Visual Studio Code, ripgrep"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-software-category"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Category
              </label>
              <select
                id="create-software-category"
                data-testid="software-create-category-select"
                value={category}
                onChange={(e) => setCategory(e.target.value)}
                disabled={submitting}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface)',
                  boxSizing: 'border-box',
                }}
              >
                <option value="application">Application</option>
                <option value="cli">CLI Tool</option>
                <option value="package">Package</option>
                <option value="runtime">Runtime</option>
                <option value="tool">Tool</option>
              </select>
            </div>

            <div>
              <label
                htmlFor="create-software-source"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Install Source
              </label>
              <select
                id="create-software-source"
                data-testid="software-create-source-select"
                value={installSource}
                onChange={(e) => setInstallSource(e.target.value)}
                disabled={submitting}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface)',
                  boxSizing: 'border-box',
                }}
              >
                <option value="homebrew_cask">Homebrew Cask</option>
                <option value="homebrew_formula">Homebrew Formula</option>
                <option value="macos_app">macOS App</option>
                <option value="npm_global">npm Global</option>
                <option value="pipx">pipx</option>
                <option value="manual">Manual</option>
                <option value="system">System</option>
                <option value="unknown">Unknown</option>
              </select>
            </div>
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-software-version"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Version
              </label>
              <input
                id="create-software-version"
                data-testid="software-create-version-input"
                type="text"
                value={version}
                onChange={(e) => setVersion(e.target.value)}
                disabled={submitting}
                placeholder="e.g. 1.85.0"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="create-software-arch"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Architecture
              </label>
              <input
                id="create-software-arch"
                data-testid="software-create-arch-input"
                type="text"
                value={architecture}
                onChange={(e) => setArchitecture(e.target.value)}
                disabled={submitting}
                placeholder="e.g. arm64, x86_64"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div>
            <label
              htmlFor="create-software-summary"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Summary
            </label>
            <input
              id="create-software-summary"
              data-testid="software-create-summary-input"
              type="text"
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              disabled={submitting}
              placeholder="Short description of the software"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="create-software-loc"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Install Location
              </label>
              <input
                id="create-software-loc"
                data-testid="software-create-loc-input"
                type="text"
                value={installLocation}
                onChange={(e) => setInstallLocation(e.target.value)}
                disabled={submitting}
                placeholder="/Applications/App.app or /opt/homebrew"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="create-software-exec"
                style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
              >
                Executable Path
              </label>
              <input
                id="create-software-exec"
                data-testid="software-create-exec-input"
                type="text"
                value={executablePath}
                onChange={(e) => setExecutablePath(e.target.value)}
                disabled={submitting}
                placeholder="/Applications/App.app/Contents/MacOS/app"
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  fontSize: '13px',
                  border: '1px solid var(--color-border)',
                  borderRadius: 'var(--radius-sm)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div>
            <label
              htmlFor="create-software-purpose"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Purpose (User Defined)
            </label>
            <input
              id="create-software-purpose"
              data-testid="software-create-purpose-input"
              type="text"
              value={purpose}
              onChange={(e) => setPurpose(e.target.value)}
              disabled={submitting}
              placeholder="Why this software is installed / what role it plays"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="create-software-tags"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Tags (comma separated)
            </label>
            <input
              id="create-software-tags"
              data-testid="software-create-tags-input"
              type="text"
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
              disabled={submitting}
              placeholder="editor, ide, productivity"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="create-software-notes"
              style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
            >
              Notes
            </label>
            <textarea
              id="create-software-notes"
              data-testid="software-create-notes-input"
              rows={3}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              disabled={submitting}
              placeholder="Configurations, licenses, setup steps"
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: '13px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                boxSizing: 'border-box',
                fontFamily: 'inherit',
              }}
            />
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '12px' }}>
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              style={{
                padding: '8px 16px',
                backgroundColor: 'var(--color-canvas)',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                fontSize: '13px',
                cursor: submitting ? 'not-allowed' : 'pointer',
              }}
            >
              Cancel
            </button>
            <button
              type="submit"
              data-testid="submit-create-software-button"
              disabled={submitting}
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
              {submitting ? 'Creating...' : 'Create Software Asset'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
