import React from 'react';
import type { ActiveModule, ActiveSection, AppCapabilities } from './types';

interface NavigationRailProps {
  currentSection: ActiveSection;
  currentModule: ActiveModule;
  capabilities: AppCapabilities | null;
  onSelectModule: (module: ActiveModule) => void;
  onSelectSection: (section: ActiveSection) => void;
}

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
      aria-label="Library Navigation"
      className="nav-rail"
      style={{
        width: '200px',
        backgroundColor: 'var(--color-canvas)',
        borderRight: '1px solid var(--color-border)',
        display: 'flex',
        flexDirection: 'column',
        padding: '16px 8px',
        flexShrink: 0,
      }}
    >
      <div style={{ padding: '0 8px 16px', borderBottom: '1px solid var(--color-border)' }}>
        <h1
          style={{
            fontSize: '15px',
            fontWeight: 600,
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
        aria-label="Library Sections"
        style={{ marginTop: '12px', display: 'flex', flexDirection: 'column', gap: '2px' }}
      >
        <button
          role="tab"
          data-testid="nav-all"
          aria-selected={isModuleActive('all')}
          onClick={() => onSelectModule('all')}
          style={{
            all: 'unset',
            cursor: 'pointer',
            padding: '7px 10px',
            borderRadius: 'var(--radius-sm)',
            backgroundColor: isModuleActive('all') ? 'var(--color-surface)' : 'transparent',
            color: isModuleActive('all') ? 'var(--color-mesh)' : 'var(--color-ink)',
            fontWeight: isModuleActive('all') ? 600 : 400,
            fontSize: '12px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <span>All Assets</span>
        </button>

        <button
          role="tab"
          data-testid="nav-media"
          aria-selected={isModuleActive('media')}
          onClick={() => onSelectModule('media')}
          style={{
            all: 'unset',
            cursor: 'pointer',
            padding: '7px 10px',
            borderRadius: 'var(--radius-sm)',
            backgroundColor: isModuleActive('media') ? 'var(--color-surface)' : 'transparent',
            color: isModuleActive('media') ? 'var(--color-mesh)' : 'var(--color-ink)',
            fontWeight: isModuleActive('media') ? 600 : 400,
            fontSize: '12px',
          }}
        >
          Media
        </button>

        <button
          role="tab"
          data-testid="nav-software"
          aria-selected={isModuleActive('software')}
          onClick={() => onSelectModule('software')}
          style={{
            all: 'unset',
            cursor: 'pointer',
            padding: '7px 10px',
            borderRadius: 'var(--radius-sm)',
            backgroundColor: isModuleActive('software') ? 'var(--color-surface)' : 'transparent',
            color: isModuleActive('software') ? 'var(--color-mesh)' : 'var(--color-ink)',
            fontWeight: isModuleActive('software') ? 600 : 400,
            fontSize: '12px',
          }}
        >
          Software
        </button>

        <button
          role="tab"
          data-testid="nav-services"
          aria-selected={isModuleActive('services')}
          onClick={() => onSelectModule('services')}
          style={{
            all: 'unset',
            cursor: 'pointer',
            padding: '7px 10px',
            borderRadius: 'var(--radius-sm)',
            backgroundColor: isModuleActive('services') ? 'var(--color-surface)' : 'transparent',
            color: isModuleActive('services') ? 'var(--color-mesh)' : 'var(--color-ink)',
            fontWeight: isModuleActive('services') ? 600 : 400,
            fontSize: '12px',
          }}
        >
          Services
        </button>

        <div
          style={{
            height: '1px',
            backgroundColor: 'var(--color-border)',
            margin: '8px 0',
          }}
        />

        <button
          role="tab"
          data-testid="nav-relations"
          aria-selected={currentSection === 'relations'}
          onClick={() => onSelectSection('relations')}
          style={{
            all: 'unset',
            cursor: 'pointer',
            padding: '7px 10px',
            borderRadius: 'var(--radius-sm)',
            backgroundColor:
              currentSection === 'relations' ? 'var(--color-surface)' : 'transparent',
            color:
              currentSection === 'relations' ? 'var(--color-mesh)' : 'var(--color-ink)',
            fontWeight: currentSection === 'relations' ? 600 : 400,
            fontSize: '12px',
          }}
        >
          Relations
        </button>

        <button
          role="tab"
          data-testid="nav-activity"
          aria-selected={currentSection === 'activity'}
          onClick={() => onSelectSection('activity')}
          style={{
            all: 'unset',
            cursor: 'pointer',
            padding: '7px 10px',
            borderRadius: 'var(--radius-sm)',
            backgroundColor:
              currentSection === 'activity' ? 'var(--color-surface)' : 'transparent',
            color:
              currentSection === 'activity' ? 'var(--color-mesh)' : 'var(--color-ink)',
            fontWeight: currentSection === 'activity' ? 600 : 400,
            fontSize: '12px',
          }}
        >
          Activity
        </button>
      </div>
    </nav>
  );
};
