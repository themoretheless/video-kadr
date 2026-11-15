import { concatImportFonts, concatImportMedia } from './concatProjectImport'

function outputFormat(document: { video?: unknown }) {
  const video = document.video
  if (!video || typeof video !== 'object') return undefined
  const { width, height, rateNum, rateDen } = video as Record<string, unknown>
  const positive = (value: unknown): value is number =>
    typeof value === 'number' && Number.isSafeInteger(value) && value > 0
  if (!positive(width) || !positive(height) || !positive(rateNum) || !positive(rateDen)) return undefined
  return { width, height, rateNum, rateDen }
}

/** Index only files explicitly selected by the browser directory picker. */
export async function readConcatTemplateFolder(files: readonly File[]) {
  const manifests = files.filter(file => /^[^/]+\/template\.json$/.test(file.webkitRelativePath))
  if (manifests.length !== 1) throw new Error('Выберите папку Concat с файлом template.json')
  const manifest = manifests[0]!
  if (manifest.size > 4 * 1024 * 1024) throw new Error('Файл Concat превышает 4 МиБ')
  const root = manifest.webkitRelativePath.slice(0, -'template.json'.length)
  const index = new Map<string, File>()
  for (const file of files) {
    if (!file.webkitRelativePath.startsWith(root)) continue
    const path = file.webkitRelativePath.slice(root.length)
    if (index.has(path)) throw new Error('Повторный путь в папке шаблона')
    index.set(path, file)
  }
  const document = JSON.parse(await manifest.text())
  function asset(path: unknown): File {
    if (typeof path !== 'string' || !path.startsWith('assets/') ||
        path.includes('\\') || path.split('/').some(part => !part || part === '.' || part === '..')) {
      throw new Error('Ресурс шаблона должен находиться в assets/')
    }
    const file = index.get(path)
    if (!file) throw new Error(`В папке шаблона отсутствует ${path}`)
    return file
  }
  const media = concatImportMedia(document)
  const design = media.flatMap((item, i) =>
    item.placeholder ? [] : [{ id: item.id, kind: item.kind, file: asset(document.media[i].path) }],
  )
  const fonts = concatImportFonts(document).map(font => ({ ...font, file: asset(font.path) }))
  if (fonts.reduce((size, font) => size + font.file.size, 0) > 8 * 1024 * 1024) {
    throw new Error('Шрифты проекта превышают 8 МиБ')
  }
  const cover = index.get('poster.jpg')
  const poster = cover && cover.size > 0 && cover.size <= 8 * 1024 * 1024 ? cover : undefined
  return {
    document, media, design, fonts, poster,
    format: outputFormat(document),
    slotCount: media.filter(item => item.placeholder).length,
  }
}

/** Concat lists used slots by their first appearance, then unused slots in bin order. */
export function concatTemplateSlotStats(document: unknown) {
  const data = document as { activeTimelineId?: unknown; timelines?: unknown } | null
  const timelines = Array.isArray(data?.timelines) ? data.timelines : []
  const timeline = timelines.find(item => item?.id === data?.activeTimelineId) ?? timelines[0]
  const stats = new Map<string, { first: number; seconds: number }>()
  if (!Array.isArray(timeline?.clips)) return stats

  for (const clip of timeline.clips) {
    if (typeof clip?.mediaId !== 'string') continue
    const previous = stats.get(clip.mediaId) ?? { first: Infinity, seconds: 0 }
    stats.set(clip.mediaId, {
      first: Number.isFinite(clip.start) ? Math.min(previous.first, clip.start) : previous.first,
      seconds: Number.isFinite(clip.duration) && clip.duration > 0
        ? previous.seconds + clip.duration
        : previous.seconds,
    })
  }
  return stats
}
