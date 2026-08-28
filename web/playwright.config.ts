/// <reference types="node" />

import { defineConfig, devices } from "@playwright/test";

const baseURL = process.env.FIXER_E2E_BASE_URL ?? "http://127.0.0.1:32145";
const browserChannel = process.env.FIXER_E2E_BROWSER_CHANNEL;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 60_000,
  expect: {
    timeout: 15_000,
  },
  reporter: "line",
  outputDir: "test-results",
  use: {
    baseURL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        ...(browserChannel ? { channel: browserChannel } : {}),
      },
    },
  ],
});
