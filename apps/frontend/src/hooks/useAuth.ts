import { useQuery } from "@tanstack/react-query";
import { me } from "../api/auth";

export function useAuth() {
  return useQuery({
    queryKey: ["me"],
    queryFn: me,
    retry: false,
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}