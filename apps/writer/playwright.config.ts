import { existsSync } from 'node:fs';
import { defineConfig, devices } from '@playwright/test';

const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
  ?? (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);

export default defineConfig({
  testDir: './tests',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://127.0.0.1:4173',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    acceptDownloads: true,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], launchOptions: { executablePath } },
    },
    {
      name: 'firefox',
      testMatch: ['**/step7-compat.spec.ts', '**/step75.spec.ts', '**/docx-relationships.spec.ts', '**/reliability-storage.spec.ts', '**/document-menu.spec.ts', '**/save-as.spec.ts', '**/audit-*.spec.ts', '**/reliability-audit-storage.spec.ts'],
      use: { ...devices['Desktop Firefox'] },
    },
    {
      name: 'webkit',
      testMatch: ['**/step7-compat.spec.ts', '**/step75.spec.ts', '**/docx-relationships.spec.ts', '**/reliability-*.spec.ts', '**/document-menu.spec.ts', '**/save-as.spec.ts', '**/audit-*.spec.ts'],
      use: { ...devices['Desktop Safari'] },
    },
  ],
  webServer: {
    command: 'npm run preview -- --port 4173 --strictPort',
    url: 'http://127.0.0.1:4173',
    reuseExistingServer: !process.env.CI,
  },
});
