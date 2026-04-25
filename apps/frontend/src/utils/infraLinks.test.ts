import { describe, it, expect } from "vitest";
import { infraLinks } from "./infraLinks";

describe("infraLinks", () => {
  it("returns empty array for empty attrs", () => {
    expect(infraLinks({})).toEqual([]);
  });

  it("returns empty array for unrecognised attrs", () => {
    expect(infraLinks({ "custom.attr": "value", "another.key": "x" })).toEqual([]);
  });

  it("returns a pod link when k8s.pod.name is present", () => {
    const links = infraLinks({ "k8s.pod.name": "checkout-pod-1" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("pod: checkout-pod-1");
    expect(links[0].href).toBe("/infrastructure/pod/checkout-pod-1");
  });

  it("returns a host link from host.name", () => {
    const links = infraLinks({ "host.name": "node-3" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("host: node-3");
    expect(links[0].href).toBe("/infrastructure/host/node-3");
  });

  it("falls back to host.id when host.name is absent", () => {
    const links = infraLinks({ "host.id": "h-abc123" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("host: h-abc123");
    expect(links[0].href).toBe("/infrastructure/host/h-abc123");
  });

  it("prefers host.name over host.id when both present", () => {
    const links = infraLinks({ "host.name": "node-3", "host.id": "h-abc123" });
    expect(links).toHaveLength(1);
    expect(links[0].href).toBe("/infrastructure/host/node-3");
  });

  it("falls back to container.id when container.name is absent", () => {
    const links = infraLinks({ "container.id": "c-xyz" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("container: c-xyz");
    expect(links[0].href).toBe("/infrastructure/container/c-xyz");
  });

  it("URL-encodes special characters in entity IDs", () => {
    const links = infraLinks({ "k8s.pod.name": "pod/with spaces" });
    expect(links[0].href).toBe(
      "/infrastructure/pod/" + encodeURIComponent("pod/with spaces")
    );
  });

  it("returns multiple links when multiple infra attrs are present", () => {
    const links = infraLinks({
      "k8s.pod.name": "checkout-pod-1",
      "host.name": "node-3",
      "k8s.namespace.name": "default",
      "k8s.cluster.name": "prod-cluster",
      "container.name": "checkout",
    });
    expect(links).toHaveLength(5);
    const hrefs = links.map((l) => l.href);
    expect(hrefs).toContain("/infrastructure/pod/checkout-pod-1");
    expect(hrefs).toContain("/infrastructure/host/node-3");
    expect(hrefs).toContain("/infrastructure/namespace/default");
    expect(hrefs).toContain("/infrastructure/cluster/prod-cluster");
    expect(hrefs).toContain("/infrastructure/container/checkout");
  });

  it("skips attrs whose value is not a non-empty string", () => {
    const links = infraLinks({
      "k8s.pod.name": "",
      "host.name": null as unknown as string,
      "container.name": 42 as unknown as string,
    });
    expect(links).toEqual([]);
  });
});
