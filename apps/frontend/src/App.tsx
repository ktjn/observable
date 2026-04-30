import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { useEffect } from "react";
import { router } from "./router";
import { ThemeProvider } from "./lib/theme";
import { TimeDisplayProvider } from "./lib/timeDisplay";
import {
  selfObservabilityRoute,
} from "./lib/selfObservability";
import {
  recordSelfObservabilityRouteChange,
} from "./lib/selfObservabilityRuntime";

const queryClient = new QueryClient();

export default function App() {
  useEffect(() => {
    // Record initial route and subsequent changes
    const unsubscribe = router.subscribe("onResolved", (event) => {
      recordSelfObservabilityRouteChange(
        selfObservabilityRoute,
        event.toLocation.pathname
      );
    });
    return () => unsubscribe();
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <TimeDisplayProvider>
          <RouterProvider router={router} />
        </TimeDisplayProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}
