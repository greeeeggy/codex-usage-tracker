import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/ui',
  reporter: [['list'], ['html', { open: 'never' }]],
  use: { baseURL: 'http://127.0.0.1:4173', viewport: { width: 1400, height: 1000 } },
  webServer: { command: 'npm run preview -- --host 127.0.0.1', url: 'http://127.0.0.1:4173', timeout: 30_000 },
});
