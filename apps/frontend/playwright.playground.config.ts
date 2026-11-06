import { defineConfig, devices } from "@playwright/test";

// Runs against the built playground static artifact (not the dev server),
// per docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// section 15 ("Playwright must run against the built static artifact").
const port = process.env.PLAYGROUND_PREVIEW_PORT ?? "4174";
const baseURL = `http://localhost:${port}/observable/`;
const build = process.env.PLAYGROUND_PREBUILT === "1" ? "" : "npm run build:playground && ";

export default defineConfig({
  testDir: "./e2e/playground",
  fullyParallel: true,
  retries: 0,
  reporter: "list",
  use: {
    baseURL,
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: `${build}npm run preview:playground -- --port ${port}`,
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
