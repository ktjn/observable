import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { createCatalogReadMock } from "./manage-service-catalog.v0.1.mjs";

const { scenarios, bindings } = JSON.parse(readFileSync(new URL("./manage-service-catalog.v0.1.json", import.meta.url)));
const record = {
  id: "visible", business_name: "Checkout", operational_name: "checkout-api", domain: "Commerce",
  criticality: "critical", lifecycle: "retired", classification: "internal", live: false, aliases: [], owners: [],
};

test("manage-service-catalog 0.1.0 read contract", async () => {
  let unavailable = false;
  const mock = createCatalogReadMock({
    records: [
      record, { ...record, id: "restricted", business_name: "Restricted Payments", classification: "restricted" },
    ],
    bindingsByService: { visible: bindings, restricted: [{ service_name: "restricted-api", environment: "prod" }] },
    canRead: (service) => service.classification === "internal",
    unavailable: () => unavailable,
  });
  for (const expected of scenarios) {
    unavailable = expected.unavailable ?? false;
    const path = expected.path.replace(/\$\{([^}]+)\}/g, (_, id) => id);
    const { status, body } = await mock(new URL(`http://localhost/v1/catalog/${path}`));
    assert.equal(status, expected.status, path);
    assert.equal(body.error, expected.error, path);
    assert.equal(body.business_name, expected.name, path);
    assert.equal(body.total, expected.total, path);
    assert.equal(body.message, expected.message, path);
    assert.deepEqual(body.items?.map((item) => item.business_name), expected.names, path);
    if (path.endsWith("/bindings")) {
      assert.deepEqual(body, expected.binding_set ? { bindings } : { error: expected.error }, path);
    }
  }
});
