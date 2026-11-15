import { CONCAT_CAPTION_CLIPS, readConcatTemplateCaptions } from './concatTemplateCaptions'
import { CONCAT_SUBTITLE_SIDECAR, MAX_SUBTITLE_SIDECAR_BYTES, validSubtitleDocuments } from '../subtitles/documents'
import { concatImportFonts, concatImportMedia } from './concatProjectImport'

export function concatTemplateFolders(files: readonly File[]) {
  const direct = files.find(file => /^[^/]+\/template\.json$/.test(file.webkitRelativePath))
  if (direct) return [{ name: direct.webkitRelativePath.split('/')[0]!, files, direct: true }]

  const folders = new Map<string, File[]>()
  for (const file of files) {
    const parts = file.webkitRelativePath.split('/')
    if (parts.length < 3 || !parts[0] || !parts[1]) continue
    const root = parts.slice(0, 2).join('/')
    const group = folders.get(root) ?? []
    group.push(file)
    folders.set(root, group)
  }
  return [...folders]
    .filter(([root, group]) => group.some(file => file.webkitRelativePath === `${root}/template.json`))
    .map(([root, group]) => ({ name: root.split('/')[1]!, files: group, direct: false }))
    .sort((left, right) => left.name.localeCompare(right.name))
}

function activeTimeline(document: unknown) {
  const data = document as { activeTimelineId?: unknown; timelines?: unknown } | null
  const timelines = Array.isArray(data?.timelines) ? data.timelines : []
  return timelines.find(item => item?.id === data?.activeTimelineId) ?? timelines[0]
}

function videoFields(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {}
}

function outputFormat(document: { video?: unknown }) {
  const shared = videoFields(document.video)
  const active = videoFields(activeTimeline(document)?.video)
  const { width, height, rateNum, rateDen } = { ...shared, ...active }
  const positive = (value: unknown): value is number =>
    typeof value === 'number' && Number.isSafeInteger(value) && value > 0
  if (!positive(width) || !positive(height) || !positive(rateNum) || !positive(rateDen)) return undefined
  return { width, height, rateNum, rateDen }
}

/** Index only files explicitly selected by the browser directory picker. */
export async function readConcatTemplateFolder(files: readonly File[]) {
  const direct = files.filter(file => /^[^/]+\/template\.json$/.test(file.webkitRelativePath))
  const manifests = direct.length ? direct :
    files.filter(file => /^[^/]+\/[^/]+\/template\.json$/.test(file.webkitRelativePath))
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
  const subtitleFile = index.get(CONCAT_SUBTITLE_SIDECAR)
  let subtitleDocuments
  if (subtitleFile) {
    if (subtitleFile.size > MAX_SUBTITLE_SIDECAR_BYTES) throw new Error('Пул субтитров шаблона слишком большой')
    const decoded: unknown = JSON.parse(await subtitleFile.text())
    if (!validSubtitleDocuments(decoded)) throw new Error('Некорректный пул субтитров шаблона')
    subtitleDocuments = decoded
  }
  const captionFile = index.get(CONCAT_CAPTION_CLIPS)
  const captionClips = captionFile ? await readConcatTemplateCaptions(captionFile, manifest) : undefined
  const cover = index.get('poster.jpg')
  const poster = cover && cover.size > 0 && cover.size <= 8 * 1024 * 1024 ? cover : undefined
  const stats = concatTemplateSlotStats(document)
  const slots = media.filter(item => item.placeholder)
    .map(({ id, name, kind }) => ({ id, name, kind, seconds: stats.get(id)?.seconds ?? 0 }))
    .sort((left, right) => (stats.get(left.id)?.first ?? Number.MAX_VALUE) - (stats.get(right.id)?.first ?? Number.MAX_VALUE))
  return {
    document, media, design, fonts, poster, subtitleDocuments, captionClips,
    format: outputFormat(document),
    slotCount: slots.length, slots,
  }
}

/** Concat lists used slots by their first appearance, then unused slots in bin order. */
export function concatTemplateSlotStats(document: unknown) {
  const timeline = activeTimeline(document)
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
