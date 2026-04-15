import { useQuery } from "@tanstack/react-query";
import { getGitState } from "@/shared/api/git";

export function useGitState(path: string | null | undefined, enabled = true) {
  return useQuery({
    queryKey: ["git-state", path],
    queryFn: () => getGitState(path ?? ""),
    enabled: enabled && Boolean(path),
    staleTime: Number.POSITIVE_INFINITY,
  });
}
