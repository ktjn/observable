import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { selfObservabilityRoute } from "./lib/selfObservability";
import "./styles.css";

void selfObservabilityRoute;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
