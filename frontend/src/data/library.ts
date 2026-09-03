import * as api from '$lib/api.js'
import type { MediaEntry } from '$lib/types.js'
import { queryClient, serverKeys } from './queryClient.js'

export async function fetchLibrary(token?: string | null, spaceId?: string | null): Promise<MediaEntry[]> {
  const entries = await api.getLibrary(token, spaceId)
  queryClient.setQueryData(serverKeys.library, entries)
  return entries
}

export function updateCachedLibrary(entry: MediaEntry): void {
  queryClient.setQueryData<MediaEntry[]>(serverKeys.library, (current = []) =>
    current.map((item) => (item.id === entry.id ? entry : item)),
  )
}

export function invalidateLibrary(): Promise<void> {
  return queryClient.invalidateQueries({ queryKey: serverKeys.library })
}
