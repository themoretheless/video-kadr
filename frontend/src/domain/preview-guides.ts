export type PreviewGuide = 'thirds' | 'center' | 'action' | 'title'

export const PREVIEW_GUIDES: readonly { key: PreviewGuide; label: string }[] = [
  { key: 'thirds', label: 'Трети' },
  { key: 'center', label: 'Центр' },
  { key: 'action', label: 'Action safe' },
  { key: 'title', label: 'Title safe' },
]

const GUIDE_KEYS: readonly PreviewGuide[] = PREVIEW_GUIDES.map(({ key }) => key)

export function isPreviewGuide(value: unknown): value is PreviewGuide {
  return GUIDE_KEYS.includes(value as PreviewGuide)
}

export function loadEnabledPreviewGuides(
  storage: Pick<Storage, 'getItem'>,
): PreviewGuide[] {
  try {
    const saved = JSON.parse(String(storage.getItem('ve_guides') ?? '[]')) as unknown
    return Array.isArray(saved) ? saved.filter(isPreviewGuide) : []
  } catch {
    return []
  }
}

export function saveEnabledPreviewGuides(
  storage: Pick<Storage, 'setItem' | 'removeItem'>,
  active: readonly PreviewGuide[],
): void {
  try {
    if (active.length === 0) storage.removeItem('ve_guides')
    else storage.setItem('ve_guides', JSON.stringify(active))
  } catch {
    // Non-fatal: the guides just won't persist across reloads.
  }
}
