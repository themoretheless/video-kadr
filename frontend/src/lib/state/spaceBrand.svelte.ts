import {
  getSpaceBrandKit,
  updateSpaceBrandKit,
  type BrandKitPayloadDto,
  type SpaceBrandKitDto,
} from '$lib/api.js'

const EMPTY_KIT: BrandKitPayloadDto = { colors: [], fonts: [], logoSourceIds: [] }

export const spaceBrandState = $state<SpaceBrandKitDto & { loading: boolean; error: string }>({
  spaceId: '', kit: EMPTY_KIT, revision: 0, updatedBy: null, updatedAt: null, loading: false, error: '',
})

export async function loadSpaceBrandKit(spaceId: string, token: string): Promise<void> {
  spaceBrandState.loading = true
  spaceBrandState.error = ''
  try {
    Object.assign(spaceBrandState, await getSpaceBrandKit(spaceId, token))
  } catch (cause) {
    spaceBrandState.error = cause instanceof Error ? cause.message : String(cause)
  } finally {
    spaceBrandState.loading = false
  }
}

export async function saveSpaceBrandKit(
  spaceId: string,
  token: string,
  kit: BrandKitPayloadDto,
): Promise<void> {
  spaceBrandState.loading = true
  spaceBrandState.error = ''
  try {
    Object.assign(spaceBrandState, await updateSpaceBrandKit(spaceId, token, spaceBrandState.revision, kit))
  } catch (cause) {
    spaceBrandState.error = cause instanceof Error ? cause.message : String(cause)
    throw cause
  } finally {
    spaceBrandState.loading = false
  }
}

export function clearSpaceBrandKit(): void {
  Object.assign(spaceBrandState, {
    spaceId: '', kit: { colors: [], fonts: [], logoSourceIds: [] }, revision: 0,
    updatedBy: null, updatedAt: null, loading: false, error: '',
  })
}
