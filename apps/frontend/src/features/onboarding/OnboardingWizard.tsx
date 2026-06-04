import { useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../components/ui/button";
import { Panel } from "../../components/ui/panel";
import { Badge } from "../../components/ui/badge";
import { LoadingState } from "../../components/ui/loading-state";
import { createToken, listTokens } from "../../api/tokens";
import { getFirstSignalStatus } from "../../api/setup";
import { useTenantContext } from "../../hooks/useTenantContext";
import {
  type Language,
  type WizardStep,
  clearOnboardingState,
  readOnboardingState,
  writeOnboardingState,
} from "./onboardingState";

// ── SDK install snippets ──────────────────────────────────────────────────────

export const LANGUAGE_LABELS: Record<Language, string> = {
  nodejs: "Node.js",
  python: "Python",
  java: "Java",
  go: "Go",
  ruby: "Ruby",
  dotnet: ".NET",
  other: "Other",
};

export function getInstallCommand(language: Language): string {
  switch (language) {
    case "nodejs":
      return "npm install @opentelemetry/sdk-node @opentelemetry/auto-instrumentations-node";
    case "python":
      return "pip install opentelemetry-distro opentelemetry-exporter-otlp\nopentelemetry-bootstrap -a install";
    case "java":
      return "curl -OL https://github.com/open-telemetry/opentelemetry-java-instrumentation/releases/latest/download/opentelemetry-javaagent.jar";
    case "go":
      return "go get go.opentelemetry.io/otel go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc";
    case "ruby":
      return "gem install opentelemetry-sdk opentelemetry-exporter-otlp opentelemetry-instrumentation-all";
    case "dotnet":
      return "dotnet add package OpenTelemetry.Exporter.OpenTelemetryProtocol\ndotnet add package OpenTelemetry.Extensions.Hosting";
    case "other":
      return "# Use any OTLP-compatible SDK — point it at the endpoint below.";
  }
}

export function getRunCommand(language: Language, endpoint: string, apiKey: string): string {
  const envBlock = `OTEL_EXPORTER_OTLP_ENDPOINT=${endpoint}\nOTEL_EXPORTER_OTLP_HEADERS="x-api-key=${apiKey}"`;
  switch (language) {
    case "nodejs":
      return `${envBlock} \\\nnode -r @opentelemetry/auto-instrumentations-node/register your-app.js`;
    case "python":
      return `${envBlock} \\\nopentelemetry-instrument python your_app.py`;
    case "java":
      return `${envBlock} \\\njava -javaagent:opentelemetry-javaagent.jar -jar your-app.jar`;
    case "go":
      return `# Set these environment variables before running your Go app:\n${envBlock}`;
    case "ruby":
      return `${envBlock} \\\nbundle exec opentelemetry-instrument ruby your_app.rb`;
    case "dotnet":
      return `# In appsettings.json:\n# \"Otlp\": { \"Endpoint\": \"${endpoint}\", \"Headers\": \"x-api-key=${apiKey}\" }`;
    case "other":
      return `# Configure your SDK with:\n# Endpoint: ${endpoint}\n# Header:   x-api-key: ${apiKey}`;
  }
}
