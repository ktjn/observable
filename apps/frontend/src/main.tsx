import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { selfObservabilityRoute } from "./lib/selfObservability";
import { initSelfObservabilityRuntime } from "./lib/selfObservabilityRuntime";
import "./styles.css";

// Initialize frontend self-observability runtime
initSelfObservabilityRuntime(selfObservabilityRoute);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
