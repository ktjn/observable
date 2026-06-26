import { test, expect, type Page } from "@playwright/test";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Registers a route handler that serves a minimal SSE stream with one event
 * of each type, then stays open with heartbeats. This lets tests run without
 * a real backend.
 */
async function mockSseStream(page: Page) {
  await page.route("**/events", async (route) => {
    const priceEvent = {
      event_id: "00000000-0000-0000-0000-000000000001",
      asset: "BTC",
      chain: "bitcoin",
      price_usd: 62000,
      source: "Coinbase",
      ts_unix_ms: Date.now(),
    };
    const txEvent = {
      tx_hash: "abc123def456abc123def456abc123def456abc123def456abc123def456abc1",
      value_usd: 12500,
      block_height: 850000,
      ts_unix_ms: Date.now(),
    };
    const correlatedEvent = {
      correlation_id: "00000000-0000-0000-0000-000000000002",
      asset: "BTC",
      tx_hash: txEvent.tx_hash,
      price_usd: 62000,
      lag_ms: 234,
      price_source: "Coinbase",
      ts_unix_ms: Date.now(),
    };

    const body = [
      `event: price\ndata: ${JSON.stringify(priceEvent)}\n\n`,
      `event: tx\ndata: ${JSON.stringify(txEvent)}\n\n`,
      `event: correlated\ndata: ${JSON.stringify(correlatedEvent)}\n\n`,
      ": heartbeat\n\n",
    ].join("");

    await route.fulfill({
      status: 200,
      headers: {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
        Connection: "keep-alive",
      },
      body,
    });
  });
}

async function mockMetrics(page: Page) {
  await page.route("**/metrics", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        snapshot_id: "00000000-0000-0000-0000-000000000003",
        ingest_rate: 4.2,
        correlation_lag_ms: 312,
        buffer_fill_ratio: 0.35,
        exporter_latency_ms: 18,
        error_count: 0,
        ts_unix_ms: Date.now(),
      }),
    });
  });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test.describe("Crypto Demo Dashboard", () => {
  test.beforeEach(async ({ page }) => {
    await mockSseStream(page);
    await mockMetrics(page);
  });

  test("renders all four dashboard sections", async ({ page }) => {
    await page.goto("/");

    await expect(page.getByTestId("section-live-prices")).toBeVisible();
    await expect(page.getByTestId("section-live-txs")).toBeVisible();
    await expect(page.getByTestId("section-correlation")).toBeVisible();
    await expect(page.getByTestId("section-lineage")).toBeVisible();
    await expect(page.getByTestId("section-pipeline-health")).toBeVisible();
  });

  test("PriceTicker displays incoming price event", async ({ page }) => {
    await page.goto("/");

    const ticker = page.getByTestId("price-ticker");
    await expect(ticker).toBeVisible();
    // The mock SSE sends one BTC price at $62 000 – wait for it to appear
    await expect(ticker.getByText("BTC")).toBeVisible({ timeout: 5_000 });
    await expect(ticker.getByText(/62,000/)).toBeVisible({ timeout: 5_000 });
  });

  test("TxList displays incoming transaction", async ({ page }) => {
    await page.goto("/");

    const txList = page.getByTestId("tx-list");
    await expect(txList).toBeVisible();
    await expect(txList.getByText(/12,500/)).toBeVisible({ timeout: 5_000 });
  });

  test("CorrelationScatter renders SVG when events arrive", async ({ page }) => {
    await page.goto("/");

    const scatter = page.getByTestId("correlation-scatter");
    await expect(scatter).toBeVisible();
    // After correlated events arrive the component renders an SVG
    await expect(scatter.locator("svg")).toBeVisible({ timeout: 5_000 });
  });

  test("LineageDiagram shows model cards and opens schema on click", async ({
    page,
  }) => {
    // Serve dummy markdown for the model schema fetch
    await page.route("**/pipeline.PriceEvent.v1.md", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "text/plain",
        body: "# PriceEvent\n- event_id: uuid\n- asset: string\n- price_usd: float",
      });
    });

    await page.goto("/");

    const lineage = page.getByTestId("lineage-diagram");
    await expect(lineage).toBeVisible();

    const priceEventBtn = page.getByTestId("lineage-model-PriceEvent");
    await expect(priceEventBtn).toBeVisible();

    // Click to expand schema
    await priceEventBtn.click();
    await expect(page.getByText(/event_id: uuid/)).toBeVisible({ timeout: 3_000 });

    // Click again to collapse
    await priceEventBtn.click();
    await expect(page.getByText(/event_id: uuid/)).not.toBeVisible();
  });

  test("PipelineHealth panel polls and displays metrics", async ({ page }) => {
    await page.goto("/");

    const health = page.getByTestId("pipeline-health");
    await expect(health).toBeVisible();
    // Ingest rate from mock: 4.2 ev/s
    await expect(health.getByText(/4\.2/)).toBeVisible({ timeout: 5_000 });
  });
});
