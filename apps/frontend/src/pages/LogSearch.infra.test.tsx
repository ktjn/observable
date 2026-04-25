import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { infraLinks } from "../utils/infraLinks";

describe("infra badge links from log resource_attributes", () => {
  it("produces pod link from k8s.pod.name", () => {
    const links = infraLinks({ "k8s.pod.name": "api-pod-99" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("pod: api-pod-99");
    expect(links[0].href).toBe("/infrastructure/pod/api-pod-99");
  });

  it("produces no links when resource_attributes is empty", () => {
    expect(infraLinks({})).toHaveLength(0);
  });
});

function LogRowBadges({ attrs }: { attrs: Record<string, unknown> }) {
  const links = infraLinks(attrs);
  if (!links.length) return null;
  return (
    <span aria-label="infra-badges">
      {links.map((l) => (
        <a key={l.href} href={l.href} style={{ marginLeft: 4, fontSize: 11 }}>
          {l.label}
        </a>
      ))}
    </span>
  );
}

describe("LogRowBadges component", () => {
  it("renders badges when infra attrs present", () => {
    render(
      <LogRowBadges attrs={{ "k8s.pod.name": "api-pod-99", "host.name": "node-1" }} />
    );
    expect(screen.getByRole("link", { name: "pod: api-pod-99" })).toHaveAttribute(
      "href",
      "/infrastructure/pod/api-pod-99"
    );
    expect(screen.getByRole("link", { name: "host: node-1" })).toHaveAttribute(
      "href",
      "/infrastructure/host/node-1"
    );
  });

  it("renders nothing when no infra attrs", () => {
    const { container } = render(<LogRowBadges attrs={{}} />);
    expect(container.firstChild).toBeNull();
  });
});
