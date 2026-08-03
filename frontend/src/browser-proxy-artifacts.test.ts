import 'fake-indexeddb/auto'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { browserProxyCapability, browserProxyKey, fingerprintBrowserProxy, getBrowserProxyArtifact, putBrowserProxyArtifact, resolveBrowserPreviewSource, validateBrowserProxyProbe } from './browser-proxy-artifacts'

const fingerprint = 'a'.repeat(64)
describe('browser proxy artifacts', () => {
  beforeEach(() => { vi.stubGlobal('navigator', { storage: { estimate: async () => ({ usage: 0, quota: 100_000 }) } }); vi.stubGlobal('URL', { createObjectURL: vi.fn(() => 'blob:proxy'), revokeObjectURL: vi.fn() }) })
  it('persists provenance separately and resolves preview only for current fingerprint', async () => {
    const blob = new Blob(['proxy'], { type: 'video/webm' })
    await putBrowserProxyArtifact({ descriptor: { schemaVersion: 1, key: browserProxyKey(fingerprint), sourceFingerprint: fingerprint, profileFingerprint: 'vp8-640-15fps-muted-v1', mimeType: blob.type, width: 640, height: 360, durationTicks: 2_000_000, sourceDurationTicks: 2_000_000, startTicks: 0, mappingTimeBase: 1_000_000, nominalFps: 15, hasAudio: false, sizeBytes: blob.size, artifactFingerprint: await fingerprintBrowserProxy(blob), createdAt: 1 }, blob })
    expect(await getBrowserProxyArtifact(fingerprint)).not.toBeNull()
    await expect(resolveBrowserPreviewSource({ id: 'v', url: 'blob:original', filename: 'v.mp4', duration: 2, width: 1920, height: 1080, fingerprint, acodec: null }, 'auto', true)).resolves.toMatchObject({ url: 'blob:proxy', usingProxy: true })
    await expect(resolveBrowserPreviewSource({ id: 'v', url: 'blob:original', filename: 'v.mp4', duration: 2, width: 1920, height: 1080, fingerprint: 'b'.repeat(64) }, 'auto', true)).resolves.toMatchObject({ url: 'blob:original', usingProxy: false })
  })
  it('does not silently select a muted browser proxy in auto mode for a source with audio', async () => {
    const video = { id: 'v', url: 'blob:original', filename: 'v.mp4', duration: 2, width: 1920, height: 1080, fingerprint, acodec: 'aac' }
    await expect(resolveBrowserPreviewSource(video, 'auto', true)).resolves.toMatchObject({ url: video.url, usingProxy: false, status: 'unsupported' })
    await expect(resolveBrowserPreviewSource(video, 'proxy', true)).resolves.toMatchObject({ usingProxy: true, hasAudio: false })
  })
  it('fails closed when browser audio provenance is unknown', async () => {
    const video = { id: 'v', url: 'blob:original', filename: 'v.mp4', duration: 2, width: 1920, height: 1080, fingerprint }
    await expect(resolveBrowserPreviewSource(video, 'auto', true)).resolves.toMatchObject({ url: video.url, usingProxy: false, status: 'unsupported' })
  })
  it('structurally keeps original when policy is original or capability absent', async () => {
    const video = { id: 'v', url: 'blob:original', filename: 'v.mp4', duration: 2, width: 1, height: 1, fingerprint }
    expect((await resolveBrowserPreviewSource(video, 'original', true)).url).toBe(video.url)
    expect((await resolveBrowserPreviewSource(video, 'proxy', false)).url).toBe(video.url)
  })
  it('reports unsupported when recorder exists but canvas capture is absent', () => {
    vi.stubGlobal('MediaRecorder', { isTypeSupported: vi.fn(() => true) })
    vi.stubGlobal('HTMLCanvasElement', class {})
    expect(browserProxyCapability()).toMatchObject({ supported: false, reason: expect.stringContaining('captureStream') })
  })
  it('rejects truncated duration and mismatched decoded dimensions before publication', () => {
    const expected = { duration: 10, width: 640, height: 360 }
    expect(() => validateBrowserProxyProbe({ duration: 10.04, width: 640, height: 360 }, expected)).not.toThrow()
    expect(() => validateBrowserProxyProbe({ duration: 9, width: 640, height: 360 }, expected)).toThrow('duration')
    expect(() => validateBrowserProxyProbe({ duration: 10, width: 320, height: 180 }, expected)).toThrow('validation')
  })
})
