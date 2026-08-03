import { describe, expect, it } from 'vitest'
import { browserMulticamTiming, buildBrowserMulticamFfmpegArgv, estimateBrowserMulticamResources, parseBrowserMulticamFlatten } from './browser-multicam-export'

const sha = 'a'.repeat(64)
const payload = {
  contract: 'multicam-flatten-v1', timeBase: 1_000, durationTicks: 4_000, timelineStartTick: 0, audioAngleId: 'angle-a',
  target: { width: 1920, height: 1080, fps: 25 },
  angles: [
    { id: 'angle-a', mediaId: 'media-a', assetRef: 'asset-z', fingerprint: sha, sourceOriginTick: 500, rate: { numerator: 1, denominator: 1 } },
    { id: 'angle-b', mediaId: 'media-b', assetRef: 'asset-a', fingerprint: 'b'.repeat(64), sourceOriginTick: 0, rate: { numerator: 1, denominator: 1 } },
  ],
  intervals: [
    { decisionId: 'cut-0', angleId: 'angle-a', mediaId: 'media-a', outputStartTick: 0, durationTicks: 1_500, sourceStart: { numerator: '500', denominator: 1 }, sourceEnd: { numerator: '2000', denominator: 1 } },
    { decisionId: 'cut-1', angleId: 'angle-b', mediaId: 'media-b', outputStartTick: 1_500, durationTicks: 2_500, sourceStart: { numerator: '1500', denominator: 1 }, sourceEnd: { numerator: '4000', denominator: 1 } },
  ],
}
const inputs = new Map([
  ['asset-z', { path: '/sources/z.mp4', sizeBytes: 10_000, hasAudio: true }],
  ['asset-a', { path: '/sources/a.mp4', sizeBytes: 20_000, hasAudio: true }],
])

describe('browser multicam export contract', () => {
  it('builds deterministic multi-input concat argv with fixed master audio and post filters', () => {
    const plan = buildBrowserMulticamFfmpegArgv(payload, {
      inputs, workerFs: true, memoryBudgetBytes: 512 * 1024 * 1024,
      postVideoFilters: ['eq=brightness=0.1'], postAudioFilters: ['volume=0.8'],
      output: { filename: 'edited.mp4', args: ['-c:v', 'libx264', '-c:a', 'aac'] },
    })
    expect(plan.inputAssetRefs).toEqual(['asset-a', 'asset-z'])
    expect(plan.expectedDurationSeconds).toBe(4)
    expect(plan.argv.slice(0, 4)).toEqual(['-i', '/sources/a.mp4', '-i', '/sources/z.mp4'])
    const graph = plan.argv[plan.argv.indexOf('-filter_complex') + 1]
    expect(graph).toBe('[1:v]trim=start=0.5:duration=1.5,setpts=(PTS-STARTPTS)/1,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1[v0];[0:v]trim=start=1.5:duration=2.5,setpts=(PTS-STARTPTS)/1,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1[v1];[v0][v1]concat=n=2:v=1:a=0[vflat];[vflat]eq=brightness=0.1[vout];[1:a]atrim=start=0.5:duration=4,asetpts=PTS-STARTPTS,volume=0.8[aout]')
    expect(plan.argv).toContain('[aout]')
  })

  it('rejects unknown keys, forged coverage, mismatched media and non-reduced rates', () => {
    expect(() => parseBrowserMulticamFlatten({ ...payload, shell: 'oops' })).toThrow('keys')
    expect(() => parseBrowserMulticamFlatten({ ...payload, intervals: [{ ...payload.intervals[0], outputStartTick: 1 }, payload.intervals[1]] })).toThrow('coverage')
    expect(() => parseBrowserMulticamFlatten({ ...payload, intervals: [{ ...payload.intervals[0], mediaId: 'media-b' }, payload.intervals[1]] })).toThrow('coverage')
    expect(() => parseBrowserMulticamFlatten({ ...payload, angles: [{ ...payload.angles[0], rate: { numerator: 2, denominator: 2 } }, payload.angles[1]] })).toThrow('reduced')
  })

  it('rejects timestamp tampering and unsafe injected filters or paths', () => {
    expect(() => parseBrowserMulticamFlatten({ ...payload, intervals: [{ ...payload.intervals[0], sourceEnd: { numerator: '2001', denominator: 1 } }, payload.intervals[1]] })).toThrow('rational')
    expect(() => buildBrowserMulticamFfmpegArgv(payload, { inputs, workerFs: true, memoryBudgetBytes: 1e9, postVideoFilters: ['null;movie=evil'], output: { filename: 'x.mp4', args: [] } })).toThrow('injected')
    const bad = new Map(inputs); bad.set('asset-a', { ...bad.get('asset-a')!, path: 'bad\npath' })
    expect(() => buildBrowserMulticamFfmpegArgv(payload, { inputs: bad, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })).toThrow('path')
  })

  it('deduplicates aggregate bytes and blocks bounded MEMFS or memory overflow', () => {
    const duplicate = { ...payload, angles: [...payload.angles, { ...payload.angles[1], id: 'angle-c', mediaId: 'media-c' }] }
    const parsed = parseBrowserMulticamFlatten(duplicate)
    expect(estimateBrowserMulticamResources(parsed, inputs, { workerFs: true, memoryBudgetBytes: 1e9 }).uniqueInputBytes).toBe(30_000)
    const huge = new Map(inputs); huge.set('asset-a', { ...huge.get('asset-a')!, sizeBytes: 129 * 1024 * 1024 })
    expect(estimateBrowserMulticamResources(parseBrowserMulticamFlatten(payload), huge, { workerFs: false, memoryBudgetBytes: 1e9 }).risk).toBe('blocked')
    expect(() => buildBrowserMulticamFfmpegArgv(payload, { inputs, workerFs: true, memoryBudgetBytes: 1, output: { filename: 'x.mp4', args: [] } })).toThrow('memory')
  })

  it('uses explicit no-audio policy when the fixed master has no audio stream', () => {
    const silent = new Map(inputs); silent.set('asset-z', { ...silent.get('asset-z')!, hasAudio: false })
    const plan = buildBrowserMulticamFfmpegArgv(payload, { inputs: silent, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(plan.argv).toContain('-an')
    expect(plan.argv.join(' ')).not.toContain('[1:a]')
  })

  it('renders a moved clip with leading black and delayed master audio', () => {
    const moved = { ...payload, timelineStartTick: 2_000 }
    const plan = buildBrowserMulticamFfmpegArgv(moved, {
      inputs, workerFs: true, memoryBudgetBytes: 1e9,
      output: { filename: 'moved.mp4', args: ['-c:v', 'libx264', '-c:a', 'aac'] },
    })
    const graph = plan.argv[plan.argv.indexOf('-filter_complex') + 1]
    expect(plan.expectedDurationSeconds).toBe(6)
    expect(graph).toContain('color=c=black:s=1920x1080:r=25:d=2,setsar=1[vgap]')
    expect(graph).toContain('[vgap][v0][v1]concat=n=3:v=1:a=0[vflat]')
    expect(graph).toContain('adelay=2000:all=1')
    expect(browserMulticamTiming(moved, {})).toMatchObject({ selectedSeconds: 6, outputSeconds: 6 })
  })

  it('normalizes combined trim and middle-cut on the clip-local clock', () => {
    expect(browserMulticamTiming(payload, {
      trim: { start: 0.5, end: 3.5 },
      segments: [{ start: 0, end: 1.5 }, { start: 2.5, end: 4 }],
      speed: 2,
    })).toEqual({
      trimStartSeconds: 0.5,
      trimEndSeconds: 3.5,
      cutStartSeconds: 1,
      cutEndSeconds: 2,
      selectedSeconds: 2,
      outputSeconds: 1,
    })
  })

  it('clips a middle-cut to the selected trim window', () => {
    expect(browserMulticamTiming(payload, {
      trim: { start: 2, end: 3 },
      segments: [{ start: 0, end: 1 }, { start: 2.5, end: 4 }],
    })).toMatchObject({ cutStartSeconds: 0, cutEndSeconds: 0.5, selectedSeconds: 0.5, outputSeconds: 0.5 })
  })
})
