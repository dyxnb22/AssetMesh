import React from 'react';
import { LANGUAGES, getLang, setLang, useT } from '../../i18n';
import { BackupSettings } from './BackupSettings';
import type { ThemePreference } from './types';
import './Settings.css';

interface SettingsViewProps {
  currentTheme: ThemePreference;
  onThemeChange: (theme: ThemePreference) => void;
}

export const SettingsView: React.FC<SettingsViewProps> = ({ currentTheme, onThemeChange }) => {
  const t = useT();
  const lang = getLang();
  const themes: { id: ThemePreference; label: string }[] = [
    { id: 'system', label: 'Follow system' },
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
  ];

  return (
    <div data-testid="settings-workspace" className="settings-workspace">
      <div className="settings-content">
        <h2 className="settings-title">{t('Settings')}</h2>
        <section className="settings-preferences" aria-label={t('Preferences')}>
          <div data-testid="settings-theme-section" className="settings-row">
            <span id="settings-theme-label">{t('Appearance')}</span>
            <div className="settings-segments" role="group" aria-labelledby="settings-theme-label">
              {themes.map((option) => (
                <button key={option.id} type="button" data-testid={`theme-${option.id}`}
                  aria-pressed={currentTheme === option.id} onClick={() => onThemeChange(option.id)}>
                  {t(option.label)}
                </button>
              ))}
            </div>
          </div>
          <div data-testid="settings-language-section" className="settings-row">
            <span id="settings-language-label">{t('Language')}</span>
            <div className="settings-segments" role="group" aria-labelledby="settings-language-label">
              {LANGUAGES.map((option) => (
                <button key={option.id} type="button" data-testid={`lang-${option.id}`}
                  aria-pressed={lang === option.id} onClick={() => setLang(option.id)}>
                  {option.label}
                </button>
              ))}
            </div>
          </div>
        </section>
        <BackupSettings />
      </div>
    </div>
  );
};
