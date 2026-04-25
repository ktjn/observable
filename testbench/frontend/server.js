'use strict';

const express = require('express');
const http = require('http');

const app = express();
const API_URL = process.env.SHOP_API_URL || 'http://shop-api:8000';
const PORT = parseInt(process.env.PORT || '3000', 10);

function apiGet(path) {
  return new Promise((resolve, reject) => {
    http.get(`${API_URL}${path}`, (res) => {
      let body = '';
      res.on('data', (d) => (body += d));
      res.on('end', () => resolve({ status: res.statusCode, body }));
    }).on('error', reject);
  });
}

app.get('/health', (_req, res) => res.json({ status: 'ok' }));

app.get('/', async (_req, res) => {
  let productsHtml = 'unavailable';
  try {
    const r = await apiGet('/products');
    const products = JSON.parse(r.body);
    productsHtml = products.map((p) => `<li>${p.name} — $${p.price}</li>`).join('');
  } catch (_) {}
  res.send(`<!doctype html>
<html><head><title>Shop Testbench</title></head>
<body>
<h1>Observable Test Bench — Shop</h1>
<h2>Products</h2>
<ul>${productsHtml}</ul>
<p>This page is served by shop-frontend (Node.js). Telemetry flows to Observable via the OTel collector.</p>
</body></html>`);
});

app.listen(PORT, () => console.log(`shop-frontend listening on :${PORT}`));

// Background: health-ping every 30s
setInterval(async () => {
  try {
    const r = await apiGet('/health');
    console.log(`health ping status=${r.status}`);
  } catch (e) {
    console.error(`health ping failed: ${e.message}`);
  }
}, 30_000);

// Background: random product query every 10–60s
function scheduleProductQuery() {
  const delay = (10 + Math.random() * 50) * 1000;
  setTimeout(async () => {
    try {
      const r = await apiGet('/products');
      const products = JSON.parse(r.body);
      console.log(`background product query count=${products.length}`);
    } catch (e) {
      console.error(`background product query failed: ${e.message}`);
    }
    scheduleProductQuery();
  }, delay);
}
scheduleProductQuery();
