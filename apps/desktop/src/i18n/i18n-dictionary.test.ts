import { readdirSync, readFileSync } from 'fs';
import { join } from 'path';
import { describe, expect, it } from 'vitest';
import { zh } from './zh';

// vitest runs with the desktop workspace as cwd, so the scan is rooted at `src`.

/**
 * Technical samples stay in English: paths, `e.g.` placeholder hints and product
 * names are entered as-is, so translating them would mislead the user.
 */
const PASSTHROUGH = new Set([
  '/Applications/App.app or /opt/homebrew',
  '/Applications/App.app/Contents/MacOS/app',
  '/path/to/destination-folder',
  '/path/to/exported-bundle',
  'assetmesh-export/',
  'example.com',
  'https://dashboard.example.com',
  'USD',
  'API',
  'SQLite 3 (WAL + FK)',
  'anime, fantasy, masterpiece',
  'cli, dev, tool',
  'editor, ide, productivity',
  'episodes, chapters',
  'saas, cloud, infra',
  'e.g. CLI tool for git repository management',
  'e.g. Crunchyroll',
  "e.g. Frieren: Beyond Journey's End",
  'e.g. GitHub Copilot, AWS, Cloudflare',
  'e.g. OpenAI, GitHub, AWS',
  'e.g. Pro, Business, Pay-as-you-go',
  'e.g. Steam, Crunchyroll',
  'e.g. Visual Studio Code, ripgrep',
  'e.g. arm64, x86_64',
  'e.g. asset.created',
  'e.g. personal, work',
  'e.g. user',
]);

const SRC = join(process.cwd(), 'src');

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'i18n') continue;
      out.push(...sourceFiles(full));
    } else if (/\.(ts|tsx)$/.test(entry.name) && !/\.test\./.test(entry.name)) {
      out.push(full);
    }
  }
  return out;
}

function unquote(raw: string): string {
  return raw
    .slice(1, -1)
    .replace(/\\"/g, '"')
    .replace(/\\'/g, "'");
}

const CALL = /\bt\(\s*('(?:[^'\\\n]|\\.)*'|"(?:[^"\\\n]|\\.)*")/g;

function keysInUse(): Map<string, string> {
  const found = new Map<string, string>();
  for (const file of sourceFiles(SRC)) {
    const text = readFileSync(file, 'utf8');
    for (const match of text.matchAll(CALL)) {
      found.set(unquote(match[1]), file);
    }
  }
  return found;
}

function vars(text: string): string[] {
  return [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
}

describe('zh dictionary', () => {
  const used = keysInUse();

  it('finds translation keys in the source', () => {
    expect(used.size).toBeGreaterThan(400);
  });

  it('translates every key the interface asks for', () => {
    const missing = [...used.keys()]
      .filter((k) => !PASSTHROUGH.has(k) && !(k in zh))
      .map((k) => `${JSON.stringify(k)} (${used.get(k)})`);
    expect(missing).toEqual([]);
  });

  it('keeps the placeholders of a translation in sync with its key', () => {
    const mismatched = Object.entries(zh)
      .filter(([key, value]) => vars(key).join(',') !== vars(value).join(','))
      .map(([key]) => JSON.stringify(key));
    expect(mismatched).toEqual([]);
  });

  it('has no empty translations', () => {
    const empty = Object.entries(zh)
      .filter(([, value]) => value.trim() === '')
      .map(([key]) => JSON.stringify(key));
    expect(empty).toEqual([]);
  });
});
