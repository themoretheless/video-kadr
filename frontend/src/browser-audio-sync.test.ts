import { describe, expect, it } from 'vitest'
import { AUDIO_SYNC_MAX_SAMPLES, browserAudioSyncExtractionArgs } from './browser-audio-sync'

describe('bounded browser audio sync extraction', () => {
  it('preserves delayed first-packet PTS and asks swresample to synthesize leading zeros', () => {
    const argv = browserAudioSyncExtractionArgs('/source/delayed.mp4', 'envelope.f32le')
    expect(argv).toContain('-copyts')
    expect(argv[argv.indexOf('-i') - 1]).toBe('-copyts')
    const filter = argv[argv.indexOf('-af') + 1]
    expect(filter).toContain('aresample=50:async=1:first_pts=0')
    expect(filter).toContain(`atrim=end_sample=${AUDIO_SYNC_MAX_SAMPLES}`)
    // This pair is the FFmpeg contract for a +PTS input: retain the timestamp,
    // then fill [0, first_pts) with silence instead of shifting content to 0.
  })

  it('rejects control characters in virtual paths', () => {
    expect(() => browserAudioSyncExtractionArgs('bad\ninput', 'out')).toThrow('path')
  })
})
