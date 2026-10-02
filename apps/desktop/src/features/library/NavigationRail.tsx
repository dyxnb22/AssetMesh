import React from 'react';
import type { ActiveModule, ActiveSection, AppCapabilities } from './types';
import { t } from '../../i18n';
import { Icon, type IconName } from '../../ui/Icon';

interface NavigationRailProps {
  currentSection: ActiveSection;
  currentModule: ActiveModule;
  capabilities: AppCapabilities | null;
  onSelectModule: (module: ActiveModule) => void;
  onSelectSection: (section: ActiveSection) => void;
}

const modules: { id: ActiveModule; label: string; icon: IconName }[] = [
  { id: 'all', label: 'All Assets', icon: 'library' },
  { id: 'media', label: 'Media', icon: 'media' },
  { id: 'software', label: 'Software', icon: 'software' },
  { id: 'services', label: 'Services', icon: 'services' },
  { id: 'subscriptions', label: 'Subscriptions', icon: 'subscriptions' },
  { id: 'info', label: 'Information', icon: 'info' },
];
const tools: { id: ActiveSection; label: string }[] = [
  { id: 'relations', label: 'Relations' }, { id: 'activity', label: 'Activity' },
  { id: 'duplicates', label: 'Duplicates' }, { id: 'import-export', label: 'Portable Data' },
];

export const NavigationRail: React.FC<NavigationRailProps> = ({ currentSection, currentModule, capabilities, onSelectModule, onSelectSection }) => (
  <nav aria-label={t('Library Navigation')} className="nav-rail">
    <div className="nav-brand"><h1>AssetMesh</h1></div>
    <div role="tablist" aria-label={t('Library Sections')} className="nav-sections">
      <div className="nav-group-label">{t('Library')}</div>
      {modules.map(({ id, label, icon }) => (
        <button key={id} role="tab" data-testid={`nav-${id}`} aria-selected={currentSection === 'library' && currentModule === id} onClick={() => onSelectModule(id)} className="nav-item">
          <Icon name={icon} /><span>{t(label)}</span>
        </button>
      ))}
      <div className="nav-group-label nav-group-separated">{t('Daily')}</div>
      <button role="tab" data-testid="nav-todos" aria-selected={currentSection === 'todos'} onClick={() => onSelectSection('todos')} className="nav-item"><Icon name="check" /><span>{t('To-do List')}</span></button>
      <details className="quiet-details nav-tools" open={tools.some(({ id }) => id === currentSection) || undefined}>
        <summary><Icon name="tools" />{t('Tools')}</summary>
        <div className="nav-tool-items">{tools.map(({ id, label }) => <button key={id} role="tab" data-testid={`nav-${id}`} aria-selected={currentSection === id} className="nav-item" onClick={() => onSelectSection(id)}>{t(label)}</button>)}</div>
      </details>
      <div className="nav-bottom">
        <button role="tab" data-testid="nav-settings" aria-selected={currentSection === 'settings'} onClick={() => onSelectSection('settings')} className="nav-item"><Icon name="settings" /><span>{t('Settings')}</span></button>
        {capabilities && <span className="nav-version">v{capabilities.version}</span>}
      </div>
    </div>
  </nav>
);
