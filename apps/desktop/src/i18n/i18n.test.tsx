import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { App } from '../app/App';
import { setTransport } from '../features/library/transport';
import { FakeDesktopTransport } from '../test/fake-transport';
import { DEFAULT_LANG, getLang, setLang, t } from './index';

describe('language store', () => {
  const initial = getLang();
  afterEach(() => act(() => setLang(initial)));

  it('translates known strings and passes unknown ones through', () => {
    setLang('zh');
    expect(t('Retry')).toBe('重试');
    expect(t('A sentence nobody translated')).toBe('A sentence nobody translated');
    setLang('en');
    expect(t('Retry')).toBe('Retry');
  });

  it('fills placeholders and keeps unknown ones intact', () => {
    setLang('zh');
    expect(t('Page {page} of {total}', { page: 2, total: 7 })).toBe('第 2 / 7 页');
    expect(t('Page {page} of {total}', { page: 2 })).toBe('第 2 / {total} 页');
  });

});

describe('Chinese interface', () => {
  const initial = getLang();
  beforeEach(() => setLang('zh'));
  afterEach(() => act(() => setLang(initial)));

  it('renders the configured default in Chinese and switches language from Settings', async () => {
    setLang(DEFAULT_LANG);
    setTransport(new FakeDesktopTransport());
    render(<App />);

    const navAll = await screen.findByTestId('nav-all');
    expect(navAll).toHaveTextContent('全部资产');
    expect(screen.getByTestId('nav-media')).toHaveTextContent('媒体');
    expect(document.documentElement.lang).toBe('zh-CN');

    fireEvent.click(screen.getByTestId('nav-settings'));
    await screen.findByTestId('settings-language-section');

    fireEvent.click(screen.getByTestId('lang-en'));
    await waitFor(() => expect(navAll).toHaveTextContent('All Assets'));
    expect(screen.getByTestId('nav-media')).toHaveTextContent('Media');
    expect(document.documentElement.lang).toBe('en-US');
    expect(localStorage.getItem('assetmesh-lang')).toBe('en');
  });

  it('translates backend setup-failure text without touching the Rust message', async () => {
    const fake = new FakeDesktopTransport();
    fake.status = { status: 'setup_failure', message: 'Application setup is required.' };
    setTransport(fake);
    render(<App />);

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('无法初始化数据库');
    expect(alert).toHaveTextContent('需要完成应用初始化。');
    expect(alert).toHaveTextContent('重试');
  });
});
