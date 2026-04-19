import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TraceDetail } from "./TraceDetail";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: false,
    },
  },
});

test("renders waterfall with spans", () => {
  const spans = [
    {
      trace_id: "abc",
      span_id: "111",
      service_name: "checkout",
      operation_name: "POST /order",
      start_time_unix_nano: 0,
      end_time_unix_nano: 5000000,
      duration_ns: 5_000_000,
      status_code: "OK",
      tenant_id: "t1",
    },
  ];
  render(
    <QueryClientProvider client={queryClient}>
      <TraceDetail traceId="abc" spans={spans} />
    </QueryClientProvider>
  );
  expect(screen.getByText(/POST \/order/)).toBeInTheDocument();
  expect(screen.getByText("5.00ms")).toBeInTheDocument();
});
