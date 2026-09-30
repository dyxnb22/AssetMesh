import React from 'react';
import type { ActiveModule, ActiveSection, AppCapabilities } from './types';
import { t } from '../../i18n';

interface NavigationRailProps {
  currentSection: ActiveSection;
  currentModule: ActiveModule;
  capabilities: AppCapabilities | null;
  onSelectModule: (module: ActiveModule) => void;
  onSelectSection: (section: ActiveSection) => void;
}

const navBtn = (active: boolean): React.CSSProperties => ({
  all: 'unset',
  cursor: 'pointer',
  padding: '7px 10px',
  borderRadius: 9,
  backgroundColor: active ? 'var(--color-mesh)' : 'transparent',
  color: active ? '#ffffff' : 'var(--color-ink)',
  fontWeight: active ? 600 : 400,
  fontSize: '12px',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  boxShadow: active ? '0 2px 8px rgba(20, 125, 120, 0.32)' : 'none',
});

const railDivider: React.CSSProperties = {
  height: '1px',
  backgroundColor: 'var(--color-border)',
  margin: '8px 2px',
};

export const NavigationRail: React.FC<NavigationRailProps> = ({
  currentSection,
  currentModule,
  capabilities,
  onSelectModule,
  onSelectSection,
}) => {
  const isModuleActive = (m: ActiveModule) =>
    currentSection === 'library' && currentModule === m;

  return (
    <nav
      aria-label={t('Library Navigation')}
      className="nav-rail"
      style={{
        width: '200px',
        backgroundColor: 'var(--glass)',
        backdropFilter: 'blur(28px) saturate(1.5)',
        border: '1px solid var(--glass-border)',
        borderRadius: 16,
        margin: '10px 0 10px 10px',
        display: 'flex',
        flexDirection: 'column',
        padding: '12px 10px',
        flexShrink: 0,
        boxShadow:
          '0 1px 0 var(--glass-highlight) inset, 0 10px 30px rgba(23, 33, 38, 0.07)',
      }}
    >
      <div style={{ padding: '2px 8px 12px' }}>
        <h1
          style={{
            fontSize: '15px',
            fontWeight: 700,
            color: 'var(--color-ink)',
            letterSpacing: '-0.01em',
          }}
        >
          AssetMesh
        </h1>
        {capabilities && (
          <div style={{ fontSize: '11px', color: 'var(--color-muted)', marginTop: '2px' }}>
            v{capabilities.version}
          </div>
        )}
      </div>

      <div
        role="tablist"
        aria-label={t('Library Sections')}
        style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}
      >
        <button
          role="tab"
          data-testid="nav-all"
          aria-selected={isModuleActive('all')}
          onClick={() => onSelectModule('all')}
          style={navBtn(isModuleActive('all'))}
        >
          <span>{t('All Assets')}</span>
        </button>

        <button
          role="tab"
          data-testid="nav-media"
          aria-selected={isModuleActive('media')}
          onClick={() => onSelectModule('media')}
          style={navBtn(isModuleActive('media'))}
        >{t('Media')}</button>

        <button
          role="tab"
          data-testid="nav-software"
          aria-selected={isModuleActive('software')}
          onClick={() => onSelectModule('software')}
          style={navBtn(isModuleActive('software'))}
        >{t('Software')}</button>

        <button
          role="tab"
          data-testid="nav-services"
          aria-selected={isModuleActive('services')}
          onClick={() => onSelectModule('services')}
          style={navBtn(isModuleActive('services'))}
        >{t('Services')}</button>

        <button
          role="tab"
          data-testid="nav-info"
          aria-selected={isModuleActive('info')}
          onClick={() => onSelectModule('info')}
          style={navBtn(isModuleActive('info'))}
        >{t('Information')}</button>

        <div aria-hidden="true" style={railDivider} />

        <button
          role="tab"
          data-testid="nav-relations"
          aria-selected={currentSection === 'relations'}
          onClick={() => onSelectSection('relations')}
          style={navBtn(currentSection === 'relations')}
        >{t('Relations')}</button>

        <button
          role="tab"
          data-testid="nav-activity"
          aria-selected={currentSection === 'activity'}
          onClick={() => onSelectSection('activity')}
          style={navBtn(currentSection === 'activity')}
        >{t('Activity')}</button>

        <button
          role="tab"
          data-testid="nav-duplicates"
          aria-selected={currentSection === 'duplicates'}
          onClick={() => onSelectSection('duplicates')}
          style={navBtn(currentSection === 'duplicates')}
        >{t('Duplicates')}</button>

        <div aria-hidden="true" style={{ flex: 1 }} />

        <div aria-hidden="true" style={railDivider} />

        <button
          role="tab"
          data-testid="nav-import-export"
          aria-selected={currentSection === 'import-export'}
          onClick={() => onSelectSection('import-export')}
          style={navBtn(currentSection === 'import-export')}
        >{t('Portable Data')}</button>

        <button
          role="tab"
          data-testid="nav-settings"
          aria-selected={currentSection === 'settings'}
          onClick={() => onSelectSection('settings')}
          style={navBtn(currentSection === 'settings')}
        >{t('Settings')}</button>
      </div>
    </nav>
  );
};
