import { api } from "./api";

export const authStatusQuery = () => ({
  queryKey: ["auth", "status"] as const,
  queryFn: () => api.authStatus(),
  staleTime: Infinity,
});
