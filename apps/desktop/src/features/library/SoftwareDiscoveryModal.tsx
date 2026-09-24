import React, { useEffect, useState } from 'react';
import { Badge } from '../../ui/Badge';
import { getTransport, normalizeDesktopError } from './transport';
import type { ClassifiedCandidateDto, DesktopError, MutationReceiptDto } from './types';
import { t } from '../../i18n';

interface SoftwareDiscoveryModalProps {
  isOpen: boolean;
  onClose: () => void;
  onAdopted: (assetId: string) => void;
}

export const SoftwareDiscoveryModal: React.FC<SoftwareDiscoveryModalProps> = ({
  isOpen,
  onClose,
  onAdopted,
}) => {
  const [loading, setLoading] = useState(false);
  const [candidates, setCandidates] = useState<ClassifiedCandidateDto[]>([]);
  const [selectedCandidate, setSelectedCandidate] = useState<ClassifiedCandidateDto | null>(null);
  const [purpose, setPurpose] = useState('');
  const [notes, setNotes] = useState('');
  const [tagsInput, setTagsInput] = useState('');
  const [target, setTarget] = useState<'auto' | 'create_new'>('auto');

  const [adopting, setAdopting] = useState(false);
  const [error, setError] = useState<DesktopError | null>(null);
  const [receipt, setReceipt] = useState<MutationReceiptDto | null>(null);

  const transport = getTransport();

  const runDiscovery = async () => {
    setLoading(true);
    setError(null);
    setSelectedCandidate(null);
    setReceipt(null);
    try {
      const results = await transport.softwareDiscover();
      setCandidates(results);
      if (results.length > 0) {
        setSelectedCandidate(results[0]);
      }
    } catch (err) {
      setError(normalizeDesktopError(err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (isOpen) {
      runDiscovery();
    }
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !adopting) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose, adopting]);

  if (!isOpen) return null;

  const handleAdopt = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedCandidate || adopting) return;

    setAdopting(true);
    setError(null);
    setReceipt(null);

    const tags = tagsInput
      .split(',')
      .map((t) => t.trim().toLowerCase())
      .filter((t) => t.length > 0);

    try {
      const existingId = target === 'auto' && selectedCandidate.disposition === 'exact_match'
        ? selectedCandidate.matched_asset_ids[0] : undefined;
      const expectedRevision = existingId
        ? (await transport.getAsset(existingId)).revision
        : undefined;
      const result = await transport.softwareCommand({
        action: 'adopt_candidate',
        candidate: selectedCandidate.candidate,
        target,
        expected_revision: expectedRevision,
        purpose: purpose.trim() || undefined,
        notes: notes.trim() || undefined,
        tags: tags.length > 0 ? tags : undefined,
      });

      setReceipt(result);
      if (result.asset_ids.length > 0) {
        onAdopted(result.asset_ids[0]);
      }
    } catch (err) {
      setError(normalizeDesktopError(err));
    } finally {
      setAdopting(false);
    }
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t('Software Discovery & Adoption')}
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
        if (e.target === e.currentTarget && !adopting) {
          onClose();
        }
      }}
    >
      <div
        style={{
          width: '100%',
          maxWidth: '840px',
          height: '80vh',
          backgroundColor: 'var(--color-surface)',
          borderRadius: 'var(--radius-lg)',
          border: '1px solid var(--color-border)',
          boxShadow: '0 8px 32px rgba(0,0,0,0.2)',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        {/* Header */}
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
            <Badge variant="mesh">{t('Discovery')}</Badge>
            <h3 style={{ margin: 0, fontSize: '16px', fontWeight: 600, color: 'var(--color-ink)' }}>{t('Software Discovery & Adoption')}</h3>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button
              onClick={runDiscovery}
              disabled={loading || adopting}
              data-testid="refresh-discovery-button"
              style={{
                padding: '6px 12px',
                fontSize: '12px',
                border: '1px solid var(--color-border)',
                borderRadius: 'var(--radius-sm)',
                backgroundColor: 'var(--color-canvas)',
                cursor: loading || adopting ? 'not-allowed' : 'pointer',
              }}
            >
              {loading ? t('Scanning...') : t('Rescan')}
            </button>
            <button
              onClick={onClose}
              disabled={adopting}
              aria-label={t('Close discovery modal')}
              style={{
                background: 'none',
                border: 'none',
                cursor: adopting ? 'not-allowed' : 'pointer',
                fontSize: '16px',
                color: 'var(--color-muted)',
              }}
            >
              ✕
            </button>
          </div>
        </div>

        {/* Content split */}
        <div style={{ display: 'flex', flex: 1, minHeight: 0 }}>
          {/* Candidates list */}
          <div
            style={{
              width: '45%',
              borderRight: '1px solid var(--color-border)',
              overflowY: 'auto',
              display: 'flex',
              flexDirection: 'column',
            }}
          >
            <div
              style={{
                padding: '10px 16px',
                borderBottom: '1px solid var(--color-border-subtle)',
                fontSize: '12px',
                color: 'var(--color-muted)',
                fontWeight: 500,
              }}
            >{t('Discovered Candidates (')}{candidates.length})
            </div>

            {loading && (
              <div style={{ padding: '24px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '13px' }}>{t('Scanning system providers (macOS Apps, Homebrew, CLI tools)...')}</div>
            )}

            {!loading && candidates.length === 0 && (
              <div style={{ padding: '24px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '13px' }}>{t('No software candidates discovered.')}</div>
            )}

            {!loading &&
              candidates.map((c, idx) => {
                const isSelected = selectedCandidate?.candidate.display_name === c.candidate.display_name;
                const isNew = c.disposition === 'new';
                const isExact = c.disposition === 'exact_match';

                return (
                  <div
                    key={`${c.candidate.provider}-${c.candidate.display_name}-${idx}`}
                    data-testid={`discovery-candidate-${c.candidate.display_name}`}
                    onClick={() => {
                      setSelectedCandidate(c);
                      setReceipt(null);
                    }}
                    style={{
                      padding: '12px 16px',
                      borderBottom: '1px solid var(--color-border-subtle)',
                      backgroundColor: isSelected ? 'var(--color-surface-hover)' : 'transparent',
                      cursor: 'pointer',
                    }}
                  >
                    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '4px' }}>
                      <span style={{ fontWeight: 600, fontSize: '13px', color: 'var(--color-ink)' }}>
                        {c.candidate.display_name}
                      </span>
                      <Badge variant={isNew ? 'mesh' : isExact ? 'muted' : 'attention'}>
                        {c.disposition}
                      </Badge>
                    </div>
                    <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>
                      <span>{c.candidate.provider}</span>
                      {c.candidate.version && <span> · v{c.candidate.version}</span>}
                      <span> · {t(c.candidate.category)}</span>
                    </div>
                  </div>
                );
              })}
          </div>

          {/* Adoption inspector & form */}
          <div style={{ width: '55%', overflowY: 'auto', padding: '20px', display: 'flex', flexDirection: 'column', gap: '14px' }}>
            {selectedCandidate ? (
              <form onSubmit={handleAdopt} data-testid="adopt-candidate-form">
                <div style={{ marginBottom: '16px' }}>
                  <h4 style={{ margin: '0 0 4px', fontSize: '15px', color: 'var(--color-ink)' }}>
                    {selectedCandidate.candidate.display_name}
                  </h4>
                  <div style={{ fontSize: '12px', color: 'var(--color-muted)' }}>{t('Provider: ')}{selectedCandidate.candidate.provider}{t(' | Source: ')}{selectedCandidate.candidate.install_source}
                  </div>
                  {selectedCandidate.candidate.install_location && (
                    <div style={{ fontSize: '11px', color: 'var(--color-muted)', marginTop: '2px', wordBreak: 'break-all' }}>{t('Location: ')}{selectedCandidate.candidate.install_location}
                    </div>
                  )}
                </div>

                {error && (
                  <div
                    role="alert"
                    data-testid="adopt-error"
                    style={{
                      padding: '10px 14px',
                      backgroundColor: 'var(--color-danger-bg, #fdf2f2)',
                      border: '1px solid var(--color-danger)',
                      borderRadius: 'var(--radius-md)',
                      color: 'var(--color-danger)',
                      fontSize: '12px',
                      marginBottom: '12px',
                    }}
                  >
                    <div style={{ fontWeight: 600 }}>{t(error.category)}</div>
                    <div>{t(error.message)}</div>
                  </div>
                )}

                {receipt && (
                  <div
                    role="status"
                    data-testid="adopt-success-receipt"
                    style={{
                      padding: '10px 14px',
                      backgroundColor: 'var(--color-success-bg, #f0fdf4)',
                      border: '1px solid var(--color-success, #22c55e)',
                      borderRadius: 'var(--radius-md)',
                      color: 'var(--color-success, #15803d)',
                      fontSize: '12px',
                      marginBottom: '12px',
                    }}
                  >
                    <div style={{ fontWeight: 600 }}>{t('Successfully Adopted!')}</div>
                    <div>{t('Operation: ')}{receipt.operation}</div>
                    <div>{t('Asset ID: ')}{receipt.asset_ids.join(', ')}</div>
                  </div>
                )}

                <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                  <div>
                    <label
                      htmlFor="adopt-target"
                      style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                    >{t('Adoption Target')}</label>
                    <select
                      id="adopt-target"
                      data-testid="adopt-target-select"
                      value={target}
                      onChange={(e) => setTarget(e.target.value as 'auto' | 'create_new')}
                      disabled={adopting}
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
                      <option value="auto">{t('Auto (Match existing or create new)')}</option>
                      <option value="create_new">{t('Explicitly Create New Asset')}</option>
                    </select>
                  </div>

                  <div>
                    <label
                      htmlFor="adopt-purpose"
                      style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                    >{t('Purpose (User Defined)')}</label>
                    <input
                      id="adopt-purpose"
                      data-testid="adopt-purpose-input"
                      type="text"
                      value={purpose}
                      onChange={(e) => setPurpose(e.target.value)}
                      disabled={adopting}
                      placeholder={t('Why this software is used')}
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
                      htmlFor="adopt-tags"
                      style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                    >{t('Tags (comma separated)')}</label>
                    <input
                      id="adopt-tags"
                      data-testid="adopt-tags-input"
                      type="text"
                      value={tagsInput}
                      onChange={(e) => setTagsInput(e.target.value)}
                      disabled={adopting}
                      placeholder={t('cli, dev, tool')}
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
                      htmlFor="adopt-notes"
                      style={{ display: 'block', fontSize: '12px', fontWeight: 500, marginBottom: '4px' }}
                    >{t('Notes')}</label>
                    <textarea
                      id="adopt-notes"
                      data-testid="adopt-notes-input"
                      rows={3}
                      value={notes}
                      onChange={(e) => setNotes(e.target.value)}
                      disabled={adopting}
                      placeholder={t('User notes or configuration')}
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
                      type="submit"
                      data-testid="submit-adopt-button"
                      disabled={adopting}
                      style={{
                        padding: '8px 16px',
                        backgroundColor: 'var(--color-mesh)',
                        color: '#fff',
                        border: 'none',
                        borderRadius: 'var(--radius-sm)',
                        fontSize: '13px',
                        fontWeight: 500,
                        cursor: adopting ? 'not-allowed' : 'pointer',
                      }}
                    >
                      {adopting ? t('Adopting...') : t('Adopt into Library')}
                    </button>
                  </div>
                </div>
              </form>
            ) : (
              <div style={{ padding: '32px', textAlign: 'center', color: 'var(--color-muted)', fontSize: '13px' }}>{t('Select a candidate from the left to view details and adopt.')}</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
