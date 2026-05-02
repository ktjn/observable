import { useState } from "react";

export interface UseSignalSearchOptions {
  initialService?: string;
}

export interface UseSignalSearchResult {
  service: string;
  setService: (service: string) => void;
}

export function useSignalSearch({
  initialService = "",
}: UseSignalSearchOptions = {}): UseSignalSearchResult {
  const [service, setService] = useState(initialService);
  return { service, setService };
}
