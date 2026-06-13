import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, test, vi } from "vitest";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import TraceCompare, { compareTracePaths, summarizeTrace } from "./TraceCompare";
import * as tracesApi from "../api/traces";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      to,
      params,
      search,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { to?: string; params?: Record<string, string>; search?: Record<string, string>; children?: React.ReactNode }) => {
      const resolvedTo = Object.entries(params ?? {}).reduce(
        (href, [key, value]) => href.replace(`$${key}`, value),
        to ?? "",
      );
      const query = search
        ? `?${new URLSearchParams(
            Object.entries(search).reduce<Record<string, string>>((acc, [key, value]) => {
              acc[key] = value;
              return acc;
            }, {}),
          ).toString()}`
        : "";
      return <a href={`${resolvedTo}${query}`} {...props}>{children}</a>;
    },
    useNavigate: () => vi.fn(),
  };
});

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function wrapper({ children }: { children: React.ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <TenantContextProvider>
        <TimeDisplayProvider>{children}</TimeDisplayProvider>
      </TenantContextProvider>
    </QueryClientProvider>
  );
}

const leftTrace: tracesApi.TraceResponse = {
  trace_id: "trace-left",
  spans: [
    {
      trace_id: "trace-left",
      tenant_id: "tenant-1",
      span_id: "left-root",
      service_name: "checkout",
      service_namespace: "prod",
      service_version: "1.0.0",
      operation_name: "POST /order",
      span_kind: "SERVER",
      start_time_unix_nano: 1_000_000,
      end_time_unix_nano: 6_000_000,
      duration_ns: 5_000_000,
      status_code: "OK",
      status_message: "",
      attributes: {},
      resource_attributes: {},
      environment: "prod",
      host_id: "host-1",
      workload: "checkout",
      deployment_id: "deploy-1",
    },
    {
      trace_id: "trace-left",
      tenant_id: "tenant-1",
      span_id: "left-child",
      parent_span_id: "left-root",
      service_name: "payments",
      service_namespace: "prod",
      service_version: "1.0.0",
      operation_name: "POST /charge",
      span_kind: "CLIENT",
      start_time_unix_nano: 2_000_000,
      end_time_unix_nano: 5_000_000,
      duration_ns: 3_000_000,
      status_code: "OK",
      status_message: "",
      attributes: {},
      resource_attributes: {},
      environment: "prod",
      host_id: "host-1",
      workload: "checkout",
      deployment_id: "deploy-1",
    },
  ],
  events: [],
};

const rightTrace: tracesApi.TraceResponse = {
  trace_id: "trace-right",
  spans: [
    {
      trace_id: "trace-right",
      tenant_id: "tenant-1",
      span_id: "right-root",
      service_name: "checkout",
      service_namespace: "prod",
      service_version: "1.0.0",
      operation_name: "POST /order",
      span_kind: "SERVER",
      start_time_unix_nano: 1_000_000,
      end_time_unix_nano: 8_500_000,
      duration_ns: 7_500_000,
      status_code: "ERROR",
      status_message: "",
      attributes: {},
      resource_attributes: {},
      environment: "prod",
      host_id: "host-2",
      workload: "checkout",
      deployment_id: "deploy-2",
    },
    {
      trace_id: "trace-right",
      tenant_id: "tenant-1",
      span_id: "right-child",
      parent_span_id: "right-root",
      service_name: "fraud",
      service_namespace: "prod",
      service_version: "1.0.0",
      operation_name: "POST /score",
      span_kind: "CLIENT",
      start_time_unix_nano: 2_000_000,
      end_time_unix_nano: 4_500_000,
      duration_ns: 2_500_000,
      status_code: "OK",
      status_message: "",
      attributes: {},
      resource_attributes: {},
      environment: "prod",
      host_id: "host-2",
      workload: "checkout",
      deployment_id: "deploy-2",
    },
    {
      trace_id: "trace-right",
      tenant_id: "tenant-1",
      span_id: "right-child-2",
      parent_span_id: "right-root",
      service_name: "cache",
      service_namespace: "prod",
      service_version: "1.0.0",
      operation_name: "GET /lookup",
      span_kind: "CLIENT",
      start_time_unix_nano: 5_000_000,
      end_time_unix_nano: 8_000_000,
      duration_ns: 3_000_000,
      status_code: "OK",
      status_message: "",
      attributes: {},
      resource_attributes: {},
      environment: "prod",
      host_id: "host-2",
      workload: "checkout",
      deployment_id: "deploy-2",
    },
  ],
  events: [],
};

beforeEach(() => {
  vi.spyOn(tracesApi, "getTrace").mockImplementation(async (_tenantId, traceId) =>
    traceId === "trace-left" ? leftTrace : rightTrace,
  );
});

test("summarizeTrace and compareTracePaths derive the diff summary", () => {
  expect(summarizeTrace(leftTrace)).toMatchObject({
    traceId: "trace-left",
    spanCount: 2,
    errorCount: 0,
    rootService: "checkout",
    rootOperation: "POST /order",
  });

  expect(compareTracePaths(leftTrace, rightTrace)).toEqual({
    shared: ["checkout · POST /order"],
    leftOnly: ["payments · POST /charge"],
    rightOnly: ["fraud · POST /score", "cache · GET /lookup"],
  });
});

test("renders the compare form and side-by-side trace summaries", async () => {
  render(<TraceCompare initialLeftTraceId="trace-left" initialRightTraceId="trace-right" />, { wrapper });

  expect(screen.getByRole("heading", { name: "Trace comparison" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Compare" })).toBeInTheDocument();
  await screen.findByText("Comparison summary");
  expect(screen.getByText("Baseline")).toBeInTheDocument();
  expect(screen.getByText("Comparison")).toBeInTheDocument();
  expect(screen.getByText("Duration delta")).toBeInTheDocument();
  expect(screen.getByText("Span delta")).toBeInTheDocument();
  expect(screen.getByText("checkout → checkout")).toBeInTheDocument();
  expect(screen.getByText("OK -> ERROR")).toBeInTheDocument();
  expect(screen.getByText("Shared path")).toBeInTheDocument();
  expect(screen.getByText("Left only")).toBeInTheDocument();
  expect(screen.getByText("Right only")).toBeInTheDocument();
  expect(screen.getByText("Trace comparison")).toBeInTheDocument();
  expect(screen.getAllByRole("link", { name: "Open full trace" })).toHaveLength(2);
});

test("renders the empty prompt when trace IDs are missing", () => {
  render(<TraceCompare />, { wrapper });

  expect(screen.getByText("Enter two trace IDs")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Compare" })).toBeDisabled();
});
