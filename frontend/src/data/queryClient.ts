import { QueryClient } from '@tanstack/query-core'

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 15_000,
      gcTime: 10 * 60_000,
      refetchOnWindowFocus: false,
    },
  },
})

export const serverKeys = {
  library: ['library'] as const,
  projects: ['projects'] as const,
  compositionProjects: ['composition-projects'] as const,
  compositionProject: (id: string) => ['composition-project', id] as const,
  job: (id: string) => ['job', id] as const,
}
