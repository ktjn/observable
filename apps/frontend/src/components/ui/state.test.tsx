import { render, screen } from "@testing-library/react";
import { ConfidenceNotice, DataState, dataStateOf, FreshnessNote, StateLabel, unfitReason, type DataStateKind, type ServiceState } from "./state";

const allStates: ServiceState[] = ["normal", "degraded", "unavailable", "planned_maintenance", "unknown"];

describe("StateLabel", () => {
  it("tells every state apart by shape and text, not colour alone (grayscale)", () => {
    const { container } = render(<>{allStates.map((s) => <StateLabel key={s} state={s} />)}</>);
    const shapes = [...container.querySelectorAll("svg[data-shape]")].map((el) => el.getAttribute("data-shape"));
    const texts = [...container.querySelectorAll("[data-state]")].map((el) => el.textContent);
    expect(new Set(shapes).size).toBe(allStates.length);
    expect(new Set(texts).size).toBe(allStates.length);
  });

  it("gives each state an accessible name with its reason (screen reader)", () => {
    render(<>{allStates.map((s) => <StateLabel key={s} state={s} reason="Stale evidence" />)}</>);
    for (const name of ["Normal", "Degraded", "Unavailable", "Planned maintenance", "Unknown"]) {
      expect(screen.getByRole("img", { name: `Health: ${name}, Stale evidence` })).toBeInTheDocument();
    }
  });

  it("never draws unknown like normal", () => {
    const { container } = render(<><StateLabel state="unknown" /><StateLabel state="normal" /></>);
    const [unknown, normal] = [...container.querySelectorAll("svg[data-shape]")];
    expect(unknown.getAttribute("data-shape")).not.toBe(normal.getAttribute("data-shape"));
    expect(unknown.parentElement?.className).not.toBe(normal.parentElement?.className);
  });
});

describe("DataState", () => {
  const kinds: DataStateKind[] = ["empty", "incomplete", "delayed", "stale", "unavailable", "restricted"];

  it("names all six data states and never shows zero", () => {
    const { container } = render(<>{kinds.map((k) => <DataState key={k} kind={k} />)}</>);
    expect(screen.getAllByRole("status")).toHaveLength(kinds.length);
    expect(container.textContent).not.toMatch(/\b0\b/);
    for (const k of kinds) expect(container.querySelector(`[data-data-state="${k}"]`)).not.toBeNull();
  });

  it("maps provider statuses, treating anything unrecognised as unavailable", () => {
    expect(dataStateOf("current")).toBeNull();
    expect(dataStateOf("missing")).toBe("empty");
    expect(dataStateOf("stale")).toBe("stale");
    expect(dataStateOf("restricted")).toBe("restricted");
    expect(dataStateOf("evaluation_unavailable")).toBe("unavailable");
    expect(dataStateOf(undefined)).toBe("unavailable");
  });
});

describe("ConfidenceNotice", () => {
  it("warns of lost confidence and names the affected functions", () => {
    render(<ConfidenceNotice confidence={{ confidence: "degraded", affected: ["detect", "ingest"] }} />);
    const notice = screen.getByRole("status", { name: "Platform confidence" });
    expect(notice).toHaveTextContent("Loss of confidence");
    expect(notice).toHaveTextContent("alert detection, telemetry ingest");
    expect(notice).toHaveTextContent("not fit for a decision");
  });

  it("treats a missing confidence as unknown, never full", () => {
    render(<ConfidenceNotice confidence={undefined} />);
    expect(screen.getByRole("status", { name: "Platform confidence" })).toHaveTextContent("Platform confidence unknown");
  });

  it("stays quiet when confidence is full", () => {
    render(<ConfidenceNotice confidence={{ confidence: "full", affected: [] }} />);
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.getByText("Platform confidence: full")).toBeInTheDocument();
  });
});

describe("FreshnessNote", () => {
  it("states scope, freshness bound, last update and refresh", () => {
    render(<FreshnessNote scope="Production · Business-critical services" freshnessSeconds={300} updatedAt={Date.UTC(2026, 10, 10, 12)} refreshSeconds={30} />);
    const note = screen.getByLabelText("View freshness");
    expect(note).toHaveTextContent("Production · Business-critical services");
    expect(note).toHaveTextContent("Evidence older than 5 min is stale");
    expect(note).toHaveTextContent("Updated");
    expect(note).toHaveTextContent("Refreshes every 30 s");
  });
});

test("unfit reasons read as words", () => {
  expect(unfitReason("evidence_stale")).toBe("evidence is stale");
  expect(unfitReason("confidence_degraded")).toBe("platform confidence is degraded");
});
