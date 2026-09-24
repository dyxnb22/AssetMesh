// Real desktop E2E: WebdriverIO -> tauri-driver -> WebKitGTK -> Tauri IPC -> SQLite.
// Run on Linux after `npm run build` and `cargo build -p assetmesh-desktop`.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const appBinary = path.resolve(here, '../../target/debug/assetmesh-desktop');
let driver;
let tempDirectory;

export const config = {
  runner: 'local',
  host: '127.0.0.1',
  port: 4444,
  specs: ['./e2e/**/*.e2e.mjs'],
  maxInstances: 1,
  capabilities: [{
    browserName: 'wry',
    'tauri:options': { application: appBinary },
  }],
  framework: 'mocha',
  mochaOpts: { ui: 'bdd', timeout: 60_000 },
  reporters: ['spec'],
  waitforTimeout: 15_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 2,
  onPrepare: async () => {
    assert.equal(process.platform, 'linux', 'Real Tauri WebDriver E2E requires Linux/WebKitGTK');
    assert.ok(existsSync(appBinary), `Build the desktop binary first: ${appBinary}`);
    const probe = spawnSync('tauri-driver', ['--version'], { stdio: 'ignore' });
    assert.equal(probe.status, 0, 'Install tauri-driver before running real E2E');
    tempDirectory = mkdtempSync(path.join(tmpdir(), 'assetmesh-real-e2e-'));
    driver = spawn('tauri-driver', [], {
      stdio: 'inherit',
      env: { ...process.env, ASSETMESH_DB: path.join(tempDirectory, 'assets.db') },
    });
    driver.on('error', (error) => { throw error; });
    for (let attempt = 0; attempt < 200; attempt += 1) {
      if (driver.exitCode !== null) throw new Error('tauri-driver exited before startup');
      try {
        const response = await fetch('http://127.0.0.1:4444/status');
        if (response.ok) return;
      } catch { /* driver not ready yet */ }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    throw new Error('tauri-driver did not start within 20 seconds');
  },
  onComplete: () => {
    driver?.kill();
    if (tempDirectory) rmSync(tempDirectory, { recursive: true, force: true });
  },
};
