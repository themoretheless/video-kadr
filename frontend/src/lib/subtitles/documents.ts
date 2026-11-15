import { MAX_SRT_BYTES } from './srt'

export const CONCAT_SUBTITLE_SIDECAR = 'video-kadr-subtitles.json'
export const MAX_SUBTITLE_SIDECAR_BYTES = 6 * MAX_SRT_BYTES + 32 * 1024

export interface SubtitleDocument {
  readonly id: string
  readonly name: string
  readonly text: string
}

export function validSubtitleDocuments(value: unknown): value is readonly SubtitleDocument[] {
  if (!Array.isArray(value) || value.length > 8) return false
  const ids = new Set<string>()
  const encoder = new TextEncoder()
  let bytes = 0
  for (const item of value) {
    if (!item || typeof item.id !== 'string' || !/^[a-zA-Z0-9_.-]{1,128}$/.test(item.id) || ids.has(item.id) ||
        typeof item.name !== 'string' || !item.name.trim() || /[\p{Cc}/\\]/u.test(item.name) || encoder.encode(item.name).length > 1024 ||
        typeof item.text !== 'string' || !item.text.trim()) return false
    ids.add(item.id)
    bytes += encoder.encode(item.text).length
    if (bytes > MAX_SRT_BYTES) return false
  }
  return true
}

export function changeSubtitleDocuments(
  current: readonly SubtitleDocument[],
  change: SubtitleDocument | string,
): readonly SubtitleDocument[] {
  if (typeof change === 'string') return current.filter(item => item.id !== change)
  if (current.some(item => item.name === change.name && item.text === change.text)) return current
  const next = [...current, change]
  if (!validSubtitleDocuments(next)) throw new Error('В проекте можно хранить до 8 файлов субтитров общим размером до 2 МиБ')
  return next
}
