import * as api from './api'
import type { DerivedTask } from './browser-derived-queue'
import type { ProxyPolicy, ProxyPreviewSource } from './browser-proxy-artifacts'
import type { VideoInfo } from './types'

/** Backend preview resolution is fail-closed: no current verified descriptor
 * means the caller keeps playing the immutable original URL. */
export async function resolveBackendPreviewSource(
  video: VideoInfo,
  policy: ProxyPolicy,
  tasks: readonly DerivedTask[],
): Promise<ProxyPreviewSource> {
  if (policy === 'original') return { url: video.url, usingProxy: false, status: 'original' }
  const project = `media:${video.id}`
  const candidates = tasks.filter(item => item.kind === 'proxy' && item.state === 'succeeded' && item.consumerProjectIds.includes(project))
  if (!candidates.length) return { url: video.url, usingProxy: false, status: 'missing' }
  let stale = false
  for (const task of candidates) {
    try {
      const status = await api.getProxyStatus(task.idempotencyKey, video.id, video.fingerprint)
      if (status.state === 'ready' && status.previewUrl) {
        return { url: status.previewUrl, usingProxy: true, status: 'ready', artifactKey: task.idempotencyKey, hasAudio: Boolean(status.proxyMedia.audioCodec) }
      }
      stale = true
    } catch {
      stale = true
      await api.invalidateProxy(task.idempotencyKey).catch(() => undefined)
    }
  }
  return { url: video.url, usingProxy: false, status: stale ? 'stale' : 'missing' }
}

export async function invalidateBackendProxyArtifact(key: string): Promise<void> {
  await api.invalidateProxy(key)
}
