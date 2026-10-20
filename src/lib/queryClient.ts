import { QueryClient } from "@tanstack/react-query";

// Shared instance so non-React modules (engine bridge) can invalidate.
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 60_000,
      gcTime: 30 * 60_000,
      refetchOnWindowFocus: false,
    },
  },
});
