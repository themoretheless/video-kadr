import { describe, expect, it } from 'vitest'
import { normalizePlayerSource, playerEventKey } from '$lib/adapters/player/sources.js'
import { resolvePlaybackCapability } from './capabilities.js'
import { classifyPlaybackError, PlaybackRecoveryPolicy } from './errors.js'
import { PreviewSession, type MonotonicClock } from './previewSession.js'
import { assertPreviewPolicyDoesNotMutateExport, choosePreviewRepresentation } from './representationPolicy.js'

class FakeClock implements MonotonicClock {
  value = 0
  now(): number { return this.value }
  tick(ms: number): void { this.value += ms }
}

describe('preview player contracts', () => {
  it('runs the preview lifecycle with a monotonic fake clock and ignores stale transitions', () => {
    const clock = new FakeClock()
    const session = new PreviewSession(clock)
    clock.tick(10)
    expect(session.ready()).toMatchObject({ state: 'Ready', revision: 1, changedAtMs: 10 })
    clock.tick(5)
    expect(session.play()).toMatchObject({ state: 'Playing', revision: 2, changedAtMs: 15 })
    expect(session.play()).toMatchObject({ revision: 2 })
    clock.value = 2
    expect(session.pause()).toMatchObject({ state: 'Paused', changedAtMs: 15 })
    expect(session.drain()).toMatchObject({ state: 'Draining' })
    expect(session.play()).toMatchObject({ state: 'Draining' })
    expect(session.drained()).toMatchObject({ state: 'Idle' })
  })

  it('normalizes source kinds and reports unsupported streaming with a typed fallback', () => {
    const hls = normalizePlayerSource('https://media.example/stream.m3u8')
    expect(hls.kind).toBe('hls')
    expect(resolvePlaybackCapability(hls, { canPlayType: () => '' })).toMatchObject({
      supported: false,
      fallback: 'progressive',
    })
    const local = normalizePlayerSource('/files/proxies/a.mp4')
    expect(local).toMatchObject({ kind: 'local', immutable: true })
    expect(playerEventKey({ type: 'source-ready', source: local })).toContain('source-ready:local')
  })

  it('bounds network recovery and then selects fallback', () => {
    const policy = new PlaybackRecoveryPolicy(2, 100)
    const error = classifyPlaybackError(new Error('network timeout'))
    expect(policy.decide(error, true)).toEqual({ action: 'retry', attempt: 1, delayMs: 100 })
    expect(policy.decide(error, true)).toEqual({ action: 'retry', attempt: 2, delayMs: 200 })
    expect(policy.decide(error, true)).toEqual({ action: 'fallback', attempt: 2, delayMs: 0 })
  })

  it('chooses a seek-friendly preview representation without changing export state', () => {
    const variants = [
      { id: '360p', width: 640, bitrateKbps: 800, local: true },
      { id: '720p', width: 1280, bitrateKbps: 2_400, local: true },
      { id: 'source', width: 3840, bitrateKbps: 20_000, local: true },
    ]
    expect(choosePreviewRepresentation(variants, { viewportWidth: 1280, scrubbing: true, networkConstrained: false })?.id).toBe('360p')
    const exportSpec = Object.freeze({ width: 3840, codec: 'h265' })
    expect(assertPreviewPolicyDoesNotMutateExport(exportSpec)).toBe(exportSpec)
  })
})
