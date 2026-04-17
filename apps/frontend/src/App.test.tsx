import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import TraceSearch from "./pages/TraceSearch";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

test("renders trace search heading", () => {
  render(
    <QueryClientProvider client={queryClient}>
      <TraceSearch />
    </QueryClientProvider>
  );
  expect(screen.getByText(/Trace Explorer/i)).toBeInTheDocument();
});
