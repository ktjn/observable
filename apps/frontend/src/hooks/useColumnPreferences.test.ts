import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { useColumnPreferences } from "./useColumnPreferences";

beforeEach(() => {
  window.localStorage.clear();
});

test("seeds from defaultOrder when nothing is stored", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  expect(result.current.columnOrder).toEqual(["a", "b", "c"]);
  expect(result.current.visibleColumns).toEqual(["a", "b", "c"]);
});

test("toggling a known column hides it without changing order", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  act(() => result.current.toggleColumn("b"));

  expect(result.current.columnOrder).toEqual(["a", "b", "c"]);
  expect(result.current.visibleColumns).toEqual(["a", "c"]);
});

test("toggling a hidden column shows it again", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  act(() => result.current.toggleColumn("b"));
  act(() => result.current.toggleColumn("b"));

  expect(result.current.visibleColumns).toEqual(["a", "b", "c"]);
});

test("toggling an unknown column appends it and shows it", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b"]));

  act(() => result.current.toggleColumn("custom.field"));

  expect(result.current.columnOrder).toEqual(["a", "b", "custom.field"]);
  expect(result.current.visibleColumns).toEqual(["a", "b", "custom.field"]);
});

test("reorderColumns replaces columnOrder and preserves hidden state", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  act(() => result.current.toggleColumn("b"));
  act(() => result.current.reorderColumns(["c", "b", "a"]));

  expect(result.current.columnOrder).toEqual(["c", "b", "a"]);
  expect(result.current.visibleColumns).toEqual(["c", "a"]);
});

test("persists across remounts under the same storage key", () => {
  const first = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));
  act(() => first.result.current.toggleColumn("a"));
  act(() => first.result.current.reorderColumns(["c", "b", "a"]));
  first.unmount();

  const second = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  expect(second.result.current.columnOrder).toEqual(["c", "b", "a"]);
  expect(second.result.current.visibleColumns).toEqual(["c", "b"]);
});

test("falls back to defaultOrder when stored data is corrupt", () => {
  window.localStorage.setItem("test.columns", "not json");

  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b"]));

  expect(result.current.columnOrder).toEqual(["a", "b"]);
  expect(result.current.visibleColumns).toEqual(["a", "b"]);
});

test("falls back to defaultOrder when stored data has the wrong shape", () => {
  window.localStorage.setItem("test.columns", JSON.stringify({ columnOrder: "a,b", hiddenColumns: [] }));

  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b"]));

  expect(result.current.columnOrder).toEqual(["a", "b"]);
});
