import type { NativeNestedProject } from '$lib/api'
export const MAX_NATIVE_CLIPBOARD_BYTES=96*1024*1024+64*1024
const FORMAT='video-kadr/clip-selection'
function bounded(text: string): void {
  if (text.length>MAX_NATIVE_CLIPBOARD_BYTES || new TextEncoder().encode(text).byteLength>MAX_NATIVE_CLIPBOARD_BYTES) throw new Error('Скопированный набор слишком большой')
}
function check(project: unknown): asserts project is NativeNestedProject {
  if (!project || typeof project!=='object' || Array.isArray(project)) throw new Error('Некорректный набор клипов')
  const value=project as NativeNestedProject, nodes=value.compositions
  if (value.schemaVersion!==1 || value.root!=='clipboard' || !nodes || typeof nodes!=='object' || Object.keys(nodes).length!==1 || !Object.hasOwn(nodes,'clipboard')) throw new Error('Некорректный набор клипов')
  const node=nodes.clipboard
  if (!node || !Array.isArray(node.instances) || node.instances.length || !node.content || !Array.isArray(node.content.tracks) || (node.content.captionTracks!==undefined && !Array.isArray(node.content.captionTracks))) throw new Error('Некорректный набор клипов')
  let count=0
  for (const track of [...node.content.tracks,...node.content.captionTracks ?? []]) {
    if (!track || !Array.isArray(track.clips)) throw new Error('Некорректная дорожка в буфере')
    count+=track.clips.length
  }
  if (!count || count>512) throw new Error('В буфере должно быть от 1 до 512 клипов')
  const pending: [unknown,number][]=[[project,0]]
  while(pending.length) {
    const [entry,depth]=pending.pop()!
    if (depth>120) throw new Error('Скопированный набор имеет слишком сложную структуру')
    if (entry && typeof entry==='object') for (const child of Object.values(entry)) pending.push([child,depth+1])
  }
}
/** Text transport carries authored data and library IDs, never embeds media files. */
export function serializeNativeClipboard(project: NativeNestedProject): string {
  check(project)
  const text=JSON.stringify({format:FORMAT,version:1,project});bounded(text);return text
}
/** Native document validation is still required before publishing this parsed payload. */
export function parseNativeClipboard(text: string): NativeNestedProject {
  bounded(text)
  let envelope: unknown
  try {envelope=JSON.parse(text)}catch{throw new Error('В системном буфере нет скопированных клипов Video Kadr')}
  if (!envelope || typeof envelope!=='object' || Array.isArray(envelope)) throw new Error('В системном буфере нет скопированных клипов Video Kadr')
  const value=envelope as Record<string,unknown>
  if (value.format!==FORMAT || value.version!==1 || Object.keys(value).some(key=>!['format','version','project'].includes(key))) throw new Error('Этот формат буфера клипов не поддерживается')
  check(value.project);return value.project
}
