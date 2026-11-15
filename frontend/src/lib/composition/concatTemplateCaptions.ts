import type { NativeNestedProject } from '$lib/api'
import type { WireTextClip } from './types'

export const CONCAT_CAPTION_CLIPS = 'video-kadr-caption-clips.json'
export const MAX_CONCAT_CAPTION_BYTES = 8 * 1024 * 1024
type Captions = readonly (readonly [timelineId: string, clip: WireTextClip])[]

export async function readConcatTemplateCaptions(file: File, manifest: File): Promise<Captions | undefined> {
  if (file.size > MAX_CONCAT_CAPTION_BYTES) throw new Error('Дорожки субтитров шаблона превышают 8 МиБ')
  const sidecar: unknown = JSON.parse(await file.text())
  if (!sidecar || typeof sidecar !== 'object' || !('manifestSha256' in sidecar) ||
      typeof sidecar.manifestSha256 !== 'string' || !/^[0-9a-f]{64}$/.test(sidecar.manifestSha256) ||
      !('clips' in sidecar) || !Array.isArray(sidecar.clips)) {
    throw new Error('Некорректные дорожки субтитров шаблона')
  }
  const digest = await crypto.subtle.digest('SHA-256', await manifest.arrayBuffer())
  const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('')
  if (hash !== sidecar.manifestSha256) return undefined
  for (const entry of sidecar.clips) {
    if (!Array.isArray(entry) || entry.length !== 2 || typeof entry[0] !== 'string' ||
        !entry[1] || typeof entry[1] !== 'object' || typeof entry[1].id !== 'string') {
      throw new Error('Некорректный клип субтитров шаблона')
    }
  }
  return sidecar.clips
}

export function restoreConcatTemplateCaptions(
  project: NativeNestedProject,
  timelineIds: Readonly<Record<string, string>>,
  captions: Captions | undefined,
): NativeNestedProject {
  if (!captions?.length) return project
  const groups = new Map<string, Map<string, WireTextClip>>()
  for (const [timeline, clip] of captions) {
    const id = Object.hasOwn(timelineIds, timeline) ? timelineIds[timeline] : undefined
    if (!id || !Object.hasOwn(project.compositions, id)) throw new Error('Таймлайн субтитров отсутствует в шаблоне')
    const group = groups.get(id) ?? new Map<string, WireTextClip>()
    if (group.has(clip.id)) throw new Error('Повторный клип субтитров шаблона')
    group.set(clip.id, clip)
    groups.set(id, group)
  }
  const compositions = { ...project.compositions }
  for (const [id, group] of groups) {
    const node = compositions[id]!
    const roles = new Set(node.content.metadata?.captionTextTracks)
    const tracks = node.content.tracks.map(track => {
      if (track.kind !== 'text') return track
      const clips = track.clips.map(clip => {
        const restored = group.get(clip.id)
        if (!restored) return clip
        if (restored.text !== clip.text || restored.timelineStartTick !== clip.timelineStartTick ||
            restored.timelineEndTick !== clip.timelineEndTick) {
          throw new Error('Клип субтитров расходится с манифестом шаблона')
        }
        group.delete(clip.id)
        roles.add(track.id)
        return restored
      })
      return { ...track, clips }
    })
    if (group.size) throw new Error('Клип субтитров отсутствует в шаблоне')
    compositions[id] = { ...node, content: { ...node.content, tracks, metadata: {
      ...node.content.metadata, captionTextTracks: [...roles],
    } } }
  }
  return { ...project, compositions }
}
