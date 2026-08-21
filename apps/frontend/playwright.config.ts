import { defineConfig, devices } from "@playwright/test";

const frontendPort = process.env.FRONTEND_PORT ?? "5173";
const frontendUrl = `http://localhost:${frontendPort}`;

export default defineConfig({
  testDir: "./e2e",
  // Runs only under playwright.playground.config.ts, against a playground-mode
  // build — hash routing and the auth-redirect bypass aren't active otherwise.
  testIgnore: ["playground/**"],
  fullyParallel: true,
  retries: 0,
  reporter: "list",
  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.01,
    },
  },
  use: {
    baseURL: frontendUrl,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: `npm run dev -- --port ${frontendPort}`,
    url: frontendUrl,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
