import { useQuery } from "@tanstack/react-query";
import { me, type MeResponse } from "../api/auth";

// Plan section 9 ("Identity"): playground mode has no auth backend, so it
// provides a fixed synthetic identity rather than triggering AppShell's
// login redirect. Tenant id matches useTenantContext's seeded default.
const PLAYGROUND_USER: MeResponse = {
  user_id: "playground-user",
  email: "playground@local",
  tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "admin" }],
};

export function useAuth() {
  const isPlayground = import.meta.env.VITE_OBSERVABLE_RUNTIME === "playground";
  return useQuery({
    queryKey: ["me"],
    queryFn: isPlayground ? async () => PLAYGROUND_USER : me,
    retry: false,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}