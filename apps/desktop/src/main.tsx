import { isTauri } from '@tauri-apps/api/core';
import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './app/App';
import { recordStartupStage } from './app/startup-timing';

recordStartupStage('frontend_loaded');

if (isTauri() && /Mac/.test(navigator.platform)) document.documentElement.dataset.nativeMac = 'true';

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
