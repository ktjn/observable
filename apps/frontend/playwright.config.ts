import { defineConfig, devices } from "@playwright/test";

const frontendPort = process.env.FRONTEND_PORT ?? "5173";
const frontendUrl = `http://localhost:${frontendPort}`;
// FRONTEND_PREBUILT=1 (scripts/local-ci.sh, after `npm run build`) serves the
// built dist/ with `vite preview` instead of the on-demand-transforming dev
// server: 153 tests in ~38s instead of ~98s on 4 vCPU, and it exercises the
// production bundle.
const webServerCommand =
  process.env.FRONTEND_PREBUILT === "1"
    ? `npx vite preview --port ${frontendPort} --strictPort`
    : `npm run dev -- --port ${frontendPort}`;

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
    command: webServerCommand,
    url: frontendUrl,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
