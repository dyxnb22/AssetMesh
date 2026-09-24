import React, { useEffect, useState } from 'react';
import { getTransport, normalizeDesktopError } from './transport';
import { LANGUAGES, getLang, setLang, useT } from '../../i18n';
import type { AppSettings, ThemePreference } from './types';

interface SettingsViewProps {
  currentTheme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
}

export const SettingsView: React.FC<SettingsViewProps> = ({ currentTheme, onThemeChange }) => {
  const t = useT();
  const lang = getLang();
  const transport = getTransport();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;
    transport
      .getAppSettings()
      .then((s) => {
        if (mounted) {
          setSettings(s);
          setIsLoading(false);
        }
      })
      .catch((err) => {
        if (mounted) {
          setError(normalizeDesktopError(err).message);
          setIsLoading(false);
        }
      });

    return () => {
      mounted = false;
    };
  }, [transport]);

  return (
    <div
      data-testid="settings-workspace"
      style={{
        flex: 1,
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        overflowY: 'auto',
        backgroundColor: 'var(--color-canvas)',
        padding: '24px',
      }}
    >
      {/* Header */}
      <div
        style={{
          marginBottom: '20px',
          borderBottom: '1px solid var(--color-border)',
          paddingBottom: '16px',
        }}
      >
        <h2
          style={{
            fontSize: '18px',
            fontWeight: 600,
            color: 'var(--color-ink)',
            margin: '0 0 4px 0',
          }}
        >
          {t('Settings & Environment')}
        </h2>
        <div style={{ fontSize: '13px', color: 'var(--color-muted)' }}>
          {t(
            'Real system and runtime configurations only. No mock toggles or speculative features.'
          )}
        </div>
      </div>

      {isLoading && (
        <div style={{ padding: '20px', color: 'var(--color-muted)', fontSize: '13px' }}>
          {t('Loading environment and system settings...')}
        </div>
      )}

      {error && (
        <div
          data-testid="settings-error-banner"
          style={{
            padding: '12px',
            backgroundColor: 'rgba(182, 59, 50, 0.08)',
            border: '1px solid var(--color-danger)',
            borderRadius: 'var(--radius-sm)',
            color: 'var(--color-danger)',
            fontSize: '13px',
            marginBottom: '20px',
            maxWidth: '700px',
          }}
        >
          {t('Failed to load settings:')} {error}
        </div>
      )}

      {settings && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '24px', maxWidth: '700px' }}>
          {/* Theme Section */}
          <div
            data-testid="settings-theme-section"
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              {t('Appearance & Theme')}
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              {t(
                'Choose whether AssetMesh follows your operating system appearance or uses a fixed light or dark palette.'
              )}
            </p>

            <div style={{ display: 'flex', gap: '10px' }}>
              <button
                data-testid="theme-system"
                onClick={() => onThemeChange('system')}
                style={{
                  cursor: 'pointer',
                  padding: '8px 16px',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '12px',
                  fontWeight: currentTheme === 'system' ? 600 : 400,
                  backgroundColor:
                    currentTheme === 'system' ? 'var(--color-canvas)' : 'var(--color-surface)',
                  color: currentTheme === 'system' ? 'var(--color-mesh)' : 'var(--color-ink)',
                  border: `1px solid ${currentTheme === 'system' ? 'var(--color-mesh)' : 'var(--color-border)'}`,
                }}
              >
                {t('System ({state})', {
                  state: currentTheme === 'system' ? t('Active') : t('Select'),
                })}
              </button>
              <button
                data-testid="theme-light"
                onClick={() => onThemeChange('light')}
                style={{
                  cursor: 'pointer',
                  padding: '8px 16px',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '12px',
                  fontWeight: currentTheme === 'light' ? 600 : 400,
                  backgroundColor:
                    currentTheme === 'light' ? 'var(--color-canvas)' : 'var(--color-surface)',
                  color: currentTheme === 'light' ? 'var(--color-mesh)' : 'var(--color-ink)',
                  border: `1px solid ${currentTheme === 'light' ? 'var(--color-mesh)' : 'var(--color-border)'}`,
                }}
              >
                {t('Light')}
              </button>
              <button
                data-testid="theme-dark"
                onClick={() => onThemeChange('dark')}
                style={{
                  cursor: 'pointer',
                  padding: '8px 16px',
                  borderRadius: 'var(--radius-sm)',
                  fontSize: '12px',
                  fontWeight: currentTheme === 'dark' ? 600 : 400,
                  backgroundColor:
                    currentTheme === 'dark' ? 'var(--color-canvas)' : 'var(--color-surface)',
                  color: currentTheme === 'dark' ? 'var(--color-mesh)' : 'var(--color-ink)',
                  border: `1px solid ${currentTheme === 'dark' ? 'var(--color-mesh)' : 'var(--color-border)'}`,
                }}
              >
                {t('Dark')}
              </button>
            </div>
          </div>


          {/* Language Section */}
          <div
            data-testid="settings-language-section"
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              {t('Language')}
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              {t('Choose the language AssetMesh uses for its interface.')}
            </p>

            <div style={{ display: 'flex', gap: '10px' }}>
              {LANGUAGES.map((option) => (
                <button
                  key={option.id}
                  data-testid={`lang-${option.id}`}
                  aria-pressed={lang === option.id}
                  onClick={() => setLang(option.id)}
                  style={{
                    cursor: 'pointer',
                    padding: '8px 16px',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: '12px',
                    fontWeight: lang === option.id ? 600 : 400,
                    backgroundColor:
                      lang === option.id ? 'var(--color-canvas)' : 'var(--color-surface)',
                    color: lang === option.id ? 'var(--color-mesh)' : 'var(--color-ink)',
                    border: `1px solid ${lang === option.id ? 'var(--color-mesh)' : 'var(--color-border)'}`,
                  }}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>

          {/* Database Section */}
          <div
            data-testid="settings-db-section"
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              {t('Local Database')}
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              {t(
                'AssetMesh operates local-first over an embedded SQLite storage engine with WAL concurrency and transactional integrity.'
              )}
            </p>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
              <div>
                <div style={{ fontSize: '11px', color: 'var(--color-muted)', marginBottom: '2px' }}>
                  {t('Database Location:')}
                </div>
                <div
                  data-testid="settings-db-path"
                  style={{
                    fontSize: '12px',
                    fontFamily: 'monospace',
                    padding: '8px 12px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    wordBreak: 'break-all',
                    color: 'var(--color-ink)',
                  }}
                >
                  {settings.db_path || t('In-Memory / Unset')}
                </div>
              </div>

              <div style={{ display: 'flex', gap: '12px' }}>
                <div
                  style={{
                    flex: 1,
                    padding: '8px 12px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                  }}
                >
                  <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{t('Status')}</div>
                  <div
                    data-testid="settings-db-status"
                    style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-mesh)' }}
                  >
                    {t(settings.db_status)}
                  </div>
                </div>
                <div
                  style={{
                    flex: 1,
                    padding: '8px 12px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                  }}
                >
                  <div style={{ fontSize: '11px', color: 'var(--color-muted)' }}>{t('Storage Engine')}</div>
                  <div style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                    {t('SQLite 3 (WAL + FK)')}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Discovery Providers Section */}
          <div
            data-testid="settings-providers-section"
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              {t('Discovery Providers Setup Status')}
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              {t(
                'Software discovery uses read-only, host-native discovery providers to scan candidate applications and tools.'
              )}
            </p>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
              {settings.providers.map((prov) => (
                <div
                  key={prov.name}
                  data-testid={`provider-card-${prov.name}`}
                  style={{
                    padding: '12px 14px',
                    backgroundColor: 'var(--color-canvas)',
                    border: '1px solid var(--color-border)',
                    borderRadius: 'var(--radius-sm)',
                    display: 'flex',
                    flexDirection: 'column',
                    gap: '4px',
                  }}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <span style={{ fontSize: '13px', fontWeight: 600, color: 'var(--color-ink)' }}>
                      {prov.display_name}
                    </span>
                    <span
                      data-testid={prov.available ? 'provider-status-available' : 'provider-status-unavailable'}
                      style={{
                        fontSize: '11px',
                        fontWeight: 600,
                        padding: '2px 8px',
                        borderRadius: '10px',
                        backgroundColor: prov.available
                          ? 'rgba(20, 125, 120, 0.12)'
                          : 'rgba(102, 116, 123, 0.15)',
                        color: prov.available ? 'var(--color-mesh)' : 'var(--color-muted)',
                      }}
                    >
                      {prov.available ? t('● Available') : t('○ Not Detected')}
                    </span>
                  </div>
                  {prov.details && (
                    <div
                      style={{
                        fontSize: '12px',
                        fontFamily: 'monospace',
                        color: 'var(--color-muted)',
                      }}
                    >
                      {prov.details}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Capabilities Section */}
          <div
            data-testid="settings-capabilities-section"
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-md)',
              padding: '20px',
            }}
          >
            <h3
              style={{
                fontSize: '14px',
                fontWeight: 600,
                color: 'var(--color-ink)',
                margin: '0 0 8px 0',
              }}
            >
              {t('Application Capabilities')}
            </h3>
            <p style={{ fontSize: '13px', color: 'var(--color-muted)', margin: '0 0 16px 0' }}>
              {t(
                'Reflects the verified capabilities reported by the Phase 4/5 application core contract.'
              )}
            </p>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', fontSize: '12px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--color-muted)' }}>{t('Desktop App Version:')}</span>
                <span style={{ fontWeight: 600, color: 'var(--color-ink)' }}>v{settings.app_version}</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--color-muted)' }}>{t('Core Protocol Version:')}</span>
                <span style={{ fontWeight: 600, color: 'var(--color-ink)' }}>
                  v{settings.capabilities.version}
                </span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--color-muted)' }}>{t('Active Modules:')}</span>
                <span style={{ fontWeight: 600, color: 'var(--color-mesh)' }}>
                  {settings.capabilities.modules.map((m) => t(m)).join(', ')}
                </span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span style={{ color: 'var(--color-muted)' }}>{t('Storable Relation Types:')}</span>
                <span style={{ fontWeight: 500, color: 'var(--color-ink)' }}>
                  {t('{n} types', { n: settings.capabilities.storable_relation_types.length })}
                </span>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
