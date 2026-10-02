import { useEffect, useRef } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { t } from '../i18n';
import { Icon } from '../ui/Icon';

interface WindowToolbarProps {
  title: string;
  sidebarOpen: boolean;
  inspectorOpen: boolean;
  onToggleSidebar: () => void;
  onToggleInspector?: () => void;
  onCreate?: () => void;
  hideCreateButton?: boolean;
  search?: string;
  searchLabel?: string;
  onSearch?: (value: string) => void;
  onFilter?: () => void;
}

export function WindowToolbar({ title, sidebarOpen, inspectorOpen, onToggleSidebar, onToggleInspector, onCreate, search, searchLabel, onSearch, onFilter, hideCreateButton }: WindowToolbarProps) {
  const searchRef = useRef<HTMLInputElement>(null);
  const nativeMac = isTauri() && /Mac/.test(navigator.platform);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (document.querySelector('[aria-modal="true"]')) return;
      if ((event.metaKey || event.ctrlKey) && ['f', 'k'].includes(event.key.toLowerCase()) && onSearch) {
        event.preventDefault(); searchRef.current?.focus(); searchRef.current?.select();
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'n' && onCreate) {
        event.preventDefault(); onCreate();
      }
      if (event.key === '/' && onSearch && !(event.target instanceof HTMLElement && event.target.closest('input, textarea, select, [contenteditable="true"]'))) {
        event.preventDefault(); searchRef.current?.focus();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [onSearch, onCreate]);
  return (
    <header className={`window-toolbar${nativeMac ? ' window-toolbar-native-mac' : ''}`} data-tauri-drag-region>
      <div className="window-toolbar-leading" data-tauri-drag-region>
        <button className="icon-button" aria-label={t('Toggle sidebar')} aria-expanded={sidebarOpen} title={t('Toggle sidebar')} onClick={onToggleSidebar}><Icon name="sidebar" /></button>
      </div>
      <div className="window-toolbar-title" data-tauri-drag-region>{title}</div>
      <div className="window-toolbar-actions">
        {onSearch && <div className="toolbar-search"><Icon name="search" size={16} /><input ref={searchRef} type="search" aria-label={searchLabel} placeholder={t('Search assets (Cmd+K)...')} value={search ?? ''} onChange={(event) => onSearch(event.target.value)} onKeyDown={(event) => { if (event.key === 'Escape') { if (search) onSearch(''); else searchRef.current?.blur(); } }} />{search && <button className="icon-button" aria-label={t('Clear search')} onClick={() => onSearch('')}><Icon name="close" size={12} /></button>}</div>}
        {onFilter && <button className="icon-button" title={t('Filters')} aria-label={t('Filters')} onClick={onFilter}><Icon name="filter" /></button>}
        {onCreate && !hideCreateButton && <button className="icon-button" title={t('New asset (Cmd+N)')} aria-label={t('New asset (Cmd+N)')} onClick={onCreate}><Icon name="plus" size={20} /></button>}
        {onToggleInspector && <button className="icon-button toolbar-inspector-toggle" title={t('Toggle inspector')} aria-label={t('Toggle inspector')} aria-expanded={inspectorOpen} onClick={onToggleInspector}><Icon name="inspector" /></button>}
      </div>
    </header>
  );
}
