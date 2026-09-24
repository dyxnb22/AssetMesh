import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { App } from '../app/App';
import { setTransport } from '../features/library/transport';
import { FakeDesktopTransport } from '../test/fake-transport';
import { DEFAULT_LANG, getLang, setLang, t, useLang } from './index';

describe('language store', () => {
  const initial = getLang();
  afterEach(() => setLang(initial));

  it('ships Chinese as the default language', () => {
    expect(DEFAULT_LANG).toBe('zh');
  });

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

  it('persists the choice, and ignores stored values it does not know', () => {
    setLang('en');
    expect(localStorage.getItem('assetmesh-lang')).toBe('en');
    localStorage.setItem('assetmesh-lang', 'fr');
    expect(getLang()).toBe('en');
  });
});

function Probe() {
  const lang = useLang();
  return <span data-testid="probe">{lang === 'zh' ? '中文' : 'English'}</span>;
}

describe('useLang', () => {
  const initial = getLang();
  afterEach(() => setLang(initial));

  it('re-renders subscribers when the language changes', () => {
    render(<Probe />);
    act(() => setLang('zh'));
    expect(screen.getByTestId('probe')).toHaveTextContent('中文');
    act(() => setLang('en'));
    expect(screen.getByTestId('probe')).toHaveTextContent('English');
  });
});

describe('Chinese interface', () => {
  const initial = getLang();
  beforeEach(() => setLang('zh'));
  afterEach(() => setLang(initial));

  it('renders the chrome in Chinese and switches back from Settings', async () => {
    setTransport(new FakeDesktopTransport());
    render(<App />);

    const navAll = await screen.findByTestId('nav-all');
    expect(navAll).toHaveTextContent('全部资产');
    expect(screen.getByTestId('nav-media')).toHaveTextContent('媒体');
    expect(screen.getByTestId('nav-relations')).toHaveTextContent('关系');
    expect(document.documentElement.lang).toBe('zh-CN');

    fireEvent.click(screen.getByTestId('nav-settings'));
    const section = await screen.findByTestId('settings-language-section');
    expect(section).toHaveTextContent('语言');

    fireEvent.click(screen.getByTestId('lang-en'));
    await waitFor(() => expect(navAll).toHaveTextContent('All Assets'));
    expect(screen.getByTestId('nav-media')).toHaveTextContent('Media');
    expect(document.documentElement.lang).toBe('en-US');
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
