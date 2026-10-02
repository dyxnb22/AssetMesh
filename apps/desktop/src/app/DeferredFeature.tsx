import React, { Suspense, useEffect } from 'react';
import { t } from '../i18n';
import { useStartupStage } from './startup-timing';

interface Props {
  children: React.ReactNode;
  dialog?: boolean;
  onClose?: () => void;
}

function FeatureFallback({ dialog, onClose, failed = false }: Omit<Props, 'children'> & { failed?: boolean }) {
  useEffect(() => {
    if (!onClose) return;
    const dismiss = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose(); };
    window.addEventListener('keydown', dismiss);
    return () => window.removeEventListener('keydown', dismiss);
  }, [onClose]);

  const content = <div role={dialog ? 'dialog' : undefined} aria-modal={dialog || undefined}
    aria-label={dialog ? t('Loading...') : undefined}
    style={{ padding: 24, background: 'var(--color-surface)', color: 'var(--color-muted)', borderRadius: 'var(--radius-lg)' }}>
    <p role={failed ? 'alert' : 'status'}>{failed ? t('Could not load this view.') : t('Loading...')}</p>
    {failed && <button className="native-button" onClick={() => window.location.reload()}>{t('Reload app')}</button>}
    {onClose && <button className="native-button" onClick={onClose}>{t('Close')}</button>}
  </div>;

  return dialog ? <div style={{ position: 'fixed', inset: 0, zIndex: 130, display: 'grid', placeItems: 'center', background: 'rgba(0,0,0,.45)' }}
    onClick={(event) => { if (event.target === event.currentTarget) onClose?.(); }}>{content}</div>
    : <div style={{ flex: 1, padding: 24 }}>{content}</div>;
}

class FeatureErrorBoundary extends React.Component<Props, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  render() {
    return this.state.failed ? <FeatureFallback failed dialog={this.props.dialog} onClose={this.props.onClose} /> : this.props.children;
  }
}

/** Keep the navigation and current library visible while an optional feature loads. */
export function DeferredFeature({ children, ...fallback }: Props) {
  return <FeatureErrorBoundary {...fallback}>
    <Suspense fallback={<FeatureFallback {...fallback} />}><MountedFeature>{children}</MountedFeature></Suspense>
  </FeatureErrorBoundary>;
}

function MountedFeature({ children }: { children: React.ReactNode }) {
  useStartupStage('workspace_mounted');
  return <>{children}</>;
}
