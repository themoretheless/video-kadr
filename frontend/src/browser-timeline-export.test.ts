import { describe, expect, it } from 'vitest'
import { MEMFS_MAX_OUTPUT_BYTES } from './browser-resource-plan'
import { buildBrowserTimelineFfmpegArgv, estimateBrowserTimelineResources, parseBrowserTimelineRender } from './browser-timeline-export'

const sha = (character: string) => character.repeat(64)
const payload = {
  contract: 'timeline-render-v1', timeBase: 1_000, durationTicks: 4_000,
  target: { width: 1920, height: 1080, fps: 25 },
  clips: [
    { id: 'clip-a', assetRef: 'asset-a', fingerprint: sha('b'), mediaKind: 'video', trackKind: 'video', trackIndex: 0, audioEnabled: true, timelineStartTick: 0, durationTicks: 2_000, sourceInTick: 1_000, sourceOutTick: 3_000 },
    { id: 'clip-b', assetRef: 'asset-b', fingerprint: sha('c'), mediaKind: 'video', trackKind: 'video', trackIndex: 1, audioEnabled: true, timelineStartTick: 1_500, durationTicks: 2_500, sourceInTick: 0, sourceOutTick: 2_500 },
    { id: 'clip-c', assetRef: 'asset-c', fingerprint: sha('d'), mediaKind: 'audio', trackKind: 'audio', trackIndex: 2, audioEnabled: true, timelineStartTick: 500, durationTicks: 3_500, sourceInTick: 200, sourceOutTick: 3_700 },
  ],
}
const inputs = new Map([
  ['asset-a', { path: '/sources/a.mp4', sizeBytes: 10_000, hasAudio: true }],
  ['asset-b', { path: '/sources/b.mp4', sizeBytes: 20_000, hasAudio: true }],
  ['asset-c', { path: '/sources/c.wav', sizeBytes: 5_000, hasAudio: true }],
])

const textClip = {
  id: 'txt-1', mediaKind: 'text',
  style: { text: 'Привет\nмир', fontSizeRatio: 0.08, color: '#ffcc00', xRatio: 0.5, yRatio: 0.85, opacity: 0.9 },
  trackKind: 'video', trackIndex: 0, audioEnabled: false,
  timelineStartTick: 500, durationTicks: 1_500, sourceInTick: 0, sourceOutTick: 1_500,
}

function graphOf(plan: { argv: string[] }): string {
  return plan.argv[plan.argv.indexOf('-filter_complex') + 1]!
}

describe('browser timeline export contract', () => {
  it('compiles a deterministic overlay/amix render graph with timeline offsets', () => {
    const plan = buildBrowserTimelineFfmpegArgv(payload, {
      inputs, workerFs: true, memoryBudgetBytes: 512 * 1024 * 1024,
      postVideoFilters: ['eq=brightness=0.1'], postAudioFilters: ['volume=0.8'],
      output: { filename: 'timeline.mp4', args: ['-c:v', 'libx264', '-c:a', 'aac'] },
    })
    expect(plan.inputAssetRefs).toEqual(['asset-a', 'asset-b', 'asset-c'])
    expect(plan.expectedDurationSeconds).toBe(4)
    expect(plan.argv.slice(0, 6)).toEqual(['-i', '/sources/a.mp4', '-i', '/sources/b.mp4', '-i', '/sources/c.wav'])
    expect(graphOf(plan)).toBe([
      'color=c=black:s=1920x1080:r=25:d=4,setsar=1[vbase]',
      '[1:v]trim=start=0:duration=2.5,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1,setpts=PTS+1.5/TB[v0]',
      '[0:v]trim=start=1:duration=2,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1,setpts=PTS+0/TB[v1]',
      '[vbase][v0]overlay=eof_action=pass:format=auto[ov0]',
      '[ov0][v1]overlay=eof_action=pass:format=auto[ov1]',
      '[ov1]eq=brightness=0.1[vout]',
      '[0:a]atrim=start=1:duration=2,asetpts=PTS-STARTPTS,adelay=0:all=1[a0]',
      '[1:a]atrim=start=0:duration=2.5,asetpts=PTS-STARTPTS,adelay=1500:all=1[a1]',
      '[2:a]atrim=start=0.2:duration=3.5,asetpts=PTS-STARTPTS,adelay=500:all=1[a2]',
      '[a0][a1][a2]amix=inputs=3:duration=longest:normalize=0,aresample[aoutm]',
      '[aoutm]volume=0.8[aout]',
    ].join(';'))
    const tail = plan.argv.slice(plan.argv.indexOf('-map'))
    expect(tail).toEqual(['-map', '[vout]', '-map', '[aout]', '-t', '4', '-fs', String(MEMFS_MAX_OUTPUT_BYTES), '-c:v', 'libx264', '-c:a', 'aac', 'timeline.mp4'])
  })

  it('multiplies clip alpha via colorchannelmixer before the overlay', () => {
    const dimmed = {
      ...payload,
      clips: [payload.clips[0], { ...payload.clips[1]!, opacity: 0.4 }, payload.clips[2]],
    }
    const plan = buildBrowserTimelineFfmpegArgv(dimmed, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(graphOf(plan)).toContain('[1:v]trim=start=0:duration=2.5,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1,format=yuva444p,colorchannelmixer=aa=0.4,setpts=PTS+1.5/TB[v0]')
    const full = {
      ...payload,
      clips: [payload.clips[0], { ...payload.clips[1]!, opacity: 1 }, payload.clips[2]],
    }
    const fullPlan = buildBrowserTimelineFfmpegArgv(full, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(graphOf(fullPlan)).not.toContain('colorchannelmixer')
    const opaquePair = {
      ...payload,
      clips: [
        { ...payload.clips[0]!, id: 'clip-out', opacity: 0.5 },
        { ...payload.clips[1]!, trackIndex: 0, timelineStartTick: 1_500, opacity: 0.5, fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: 'wipeleft', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } },
        payload.clips[2],
      ],
    }
    expect(() => buildBrowserTimelineFfmpegArgv(opaquePair, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })).toThrow('opacity is not supported inside xfade pairs')
  })

  it('blends an xfade pair into one composited overlay stream with paired audio fades', () => {
    const xfade = {
      ...payload,
      clips: [
        { ...payload.clips[0]!, id: 'clip-out', fades: { videoInTicks: 0, videoInMode: 'none', videoInXfade: null, videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 500 } },
        { ...payload.clips[1]!, trackIndex: 0, timelineStartTick: 1_500, fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: 'wipeleft', videoOutTicks: 0, audioInTicks: 500, audioOutTicks: 0 } },
        payload.clips[2],
      ],
    }
    const plan = buildBrowserTimelineFfmpegArgv(xfade, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(graphOf(plan)).toBe([
      'color=c=black:s=1920x1080:r=25:d=4,setsar=1[vbase]',
      '[0:v]trim=start=1:duration=2,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1[v1out]',
      '[1:v]trim=start=0:duration=2.5,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1[v1in]',
      '[v1out][v1in]xfade=transition=wipeleft:duration=0.5:offset=1.5[v1xf]',
      '[v1xf]setpts=PTS+0/TB[v1]',
      '[vbase][v1]overlay=eof_action=pass:format=auto[ov0]',
      '[0:a]atrim=start=1:duration=2,asetpts=PTS-STARTPTS,afade=t=out:st=1.5:d=0.5,adelay=0:all=1[a0]',
      '[1:a]atrim=start=0:duration=2.5,asetpts=PTS-STARTPTS,afade=t=in:st=0:d=0.5,adelay=1500:all=1[a1]',
      '[2:a]atrim=start=0.2:duration=3.5,asetpts=PTS-STARTPTS,adelay=500:all=1[a2]',
      '[a0][a1][a2]amix=inputs=3:duration=longest:normalize=0,aresample[aoutm]',
    ].join(';'))
    expect(plan.argv.slice(plan.argv.indexOf('-map'))).toEqual(['-map', '[ov0]', '-map', '[aoutm]', '-t', '4', '-fs', String(MEMFS_MAX_OUTPUT_BYTES), 'x.mp4'])
  })

  it('rejects xfade clips without an adjacent outgoing clip or with chained pairs', () => {
    const stranded = {
      ...payload,
      clips: [{ ...payload.clips[1]!, timelineStartTick: 1_500, fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: 'wipeleft', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } }],
    }
    expect(() => buildBrowserTimelineFfmpegArgv(stranded, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })).toThrow('adjacent outgoing')
    const chained = {
      ...payload,
      durationTicks: 8_000,
      clips: [
        { ...payload.clips[0]!, id: 'clip-o', timelineStartTick: 0, durationTicks: 2_000, sourceInTick: 0, sourceOutTick: 2_000 },
        { ...payload.clips[1]!, id: 'clip-m', trackIndex: 0, timelineStartTick: 1_500, durationTicks: 2_500, fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: 'slideleft', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } },
        { ...payload.clips[1]!, id: 'clip-i', trackIndex: 0, timelineStartTick: 3_500, durationTicks: 2_500, sourceInTick: 1_000, sourceOutTick: 3_500, fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: 'wipeleft', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } },
      ],
    }
    expect(() => buildBrowserTimelineFfmpegArgv(chained, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })).toThrow('cannot chain')
  })

  it('stacks the lowest track index last and passes through silent video-only sources', () => {
    const silent = new Map(inputs)
    silent.set('asset-b', { ...silent.get('asset-b')!, hasAudio: false })
    const plan = buildBrowserTimelineFfmpegArgv(payload, { inputs: silent, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    const graph = graphOf(plan)
    expect(graph.indexOf('[v0]')).toBeLessThan(graph.indexOf('[v1]'))
    expect(graph).toContain('[ov0][v1]overlay')
    expect(graph).not.toContain('[1:a]')
    expect(graph).toContain('[a0][a1]amix=inputs=2')
  })

  it('loops image inputs and trims them to the clip window without source trims', () => {
    const still = {
      ...payload,
      clips: [{ id: 'img-1', assetRef: 'asset-i', fingerprint: sha('e'), mediaKind: 'image', trackKind: 'video', trackIndex: 0, audioEnabled: false, timelineStartTick: 1_000, durationTicks: 2_000, sourceInTick: 0, sourceOutTick: 2_000 }],
      durationTicks: 3_000,
    }
    const stillInputs = new Map([['asset-i', { path: '/sources/i.png', sizeBytes: 1_000, hasAudio: false }]])
    const plan = buildBrowserTimelineFfmpegArgv(still, { inputs: stillInputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(plan.argv.slice(0, 8)).toEqual(['-loop', '1', '-framerate', '25', '-t', '2', '-i', '/sources/i.png'])
    expect(graphOf(plan)).toContain('[0:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1,trim=duration=2,setpts=PTS-STARTPTS+1/TB[v0]')
    expect(plan.argv).toContain('-an')
  })

  it('composites a looped rasterized text layer on top without emitting its audio', () => {
    const withText = { ...payload, clips: [{ ...payload.clips[0], trackIndex: 1 }, textClip] }
    const textInputs = new Map([
      ['asset-a', inputs.get('asset-a')!],
      ['text:txt-1', { path: '/timeline-text/txt-1.png', sizeBytes: 900, hasAudio: false }],
    ])
    const plan = buildBrowserTimelineFfmpegArgv(withText, {
      inputs: textInputs, workerFs: true, memoryBudgetBytes: 1e9,
      output: { filename: 'timeline.mp4', args: [] },
    })
    expect(plan.inputAssetRefs).toEqual(['asset-a', 'text:txt-1'])
    expect(plan.argv.slice(0, 10)).toEqual([
      '-i', '/sources/a.mp4',
      '-loop', '1', '-framerate', '25', '-t', '1.5', '-i', '/timeline-text/txt-1.png',
    ])
    const graph = graphOf(plan)
    expect(graph).toContain('[1:v]fps=25,setpts=PTS-STARTPTS+0.5/TB[v1]')
    expect(graph).toContain('[ov0][v1]overlay=eof_action=pass:format=auto[ov1]')
    expect(graph).toContain('[a0]anull[aout]')
    expect(graph).not.toContain('[1:a]')
    expect(estimateBrowserTimelineResources(parseBrowserTimelineRender(withText), textInputs, { workerFs: true, memoryBudgetBytes: 1e9 }).uniqueInputBytes)
      .toBe(10_900)
  })

  it('renders crossfade alpha and fade-to-black windows with paired audio afades', () => {
    const outgoing = { ...payload.clips[0], trackIndex: 0, fades: { videoInTicks: 0, videoInMode: 'none', videoOutTicks: 500, audioInTicks: 0, audioOutTicks: 500 } }
    const incoming = {
      id: 'clip-b2', assetRef: 'asset-b', fingerprint: sha('c'), mediaKind: 'video',
      trackKind: 'video', trackIndex: 0, audioEnabled: true,
      timelineStartTick: 1_500, durationTicks: 3_000, sourceInTick: 0, sourceOutTick: 3_000,
      fades: { videoInTicks: 500, videoInMode: 'alpha', videoOutTicks: 0, audioInTicks: 500, audioOutTicks: 0 },
    }
    const crossfade = { ...payload, durationTicks: 5_000, clips: [outgoing, incoming] }
    const graph = graphOf(buildBrowserTimelineFfmpegArgv(crossfade, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } }))
    expect(graph).toContain('[1:v]trim=start=0:duration=3,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1,format=yuva444p,fade=t=in:st=0:d=0.5:alpha=1,setpts=PTS+1.5/TB[v1]')
    expect(graph).toContain('[0:a]atrim=start=1:duration=2,asetpts=PTS-STARTPTS,afade=t=out:st=1.5:d=0.5,adelay=0:all=1[a0]')
    expect(graph).toContain('[1:a]atrim=start=0:duration=3,asetpts=PTS-STARTPTS,afade=t=in:st=0:d=0.5,adelay=1500:all=1[a1]')
    const black = { ...crossfade, clips: [outgoing, { ...incoming, fades: { ...incoming.fades, videoInMode: 'black' } }] }
    const blackGraph = graphOf(buildBrowserTimelineFfmpegArgv(black, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } }))
    expect(blackGraph).toContain('[0:v]trim=start=1:duration=2,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=25,setsar=1,fade=t=out:st=1.5:d=0.5,setpts=PTS+0/TB[v0]')
    expect(blackGraph).toContain(',fade=t=in:st=0:d=0.5,setpts=PTS+1.5/TB[v1]')
    expect(blackGraph).not.toContain('yuva444p')
  })

  it('maps a single audio stream without amix and mutes on request', () => {
    const audioOnly = { ...payload, clips: [payload.clips[2]] }
    const single = buildBrowserTimelineFfmpegArgv(audioOnly, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(graphOf(single)).toContain('[a0]anull[aout]')
    const muted = buildBrowserTimelineFfmpegArgv(payload, { inputs, workerFs: true, memoryBudgetBytes: 1e9, muteAudio: true, output: { filename: 'x.mp4', args: [] } })
    expect(muted.argv).toContain('-an')
    expect(graphOf(muted)).not.toContain('atrim')
    const mutedTrack = { ...payload, clips: [{ ...payload.clips[0], audioEnabled: false }, payload.clips[1], payload.clips[2]] }
    const quiet = buildBrowserTimelineFfmpegArgv(mutedTrack, { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })
    expect(graphOf(quiet)).not.toContain('[0:a]')
    expect(graphOf(quiet)).toContain('[a0][a1]amix=inputs=2')
  })

  it('rejects unknown keys, duplicate ids, broken ranges and inconsistent kinds', () => {
    expect(() => parseBrowserTimelineRender({ ...payload, shell: 'oops' })).toThrow('keys')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [payload.clips[0], { ...payload.clips[1], id: 'clip-a' }] })).toThrow('duplicate')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], timelineStartTick: 3_000 }] })).toThrow('clip range')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], sourceOutTick: 2_500 }] })).toThrow('source duration')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: null, videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } }] })).toThrow('xfade transition name')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], fades: { videoInTicks: 0, videoInMode: 'none', videoInXfade: 'wipeleft', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } }] })).toThrow('xfade transition name')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], fades: { videoInTicks: 500, videoInMode: 'xfade', videoInXfade: 'dissolve', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 } }] })).toThrow('videoInXfade')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [payload.clips[0], { ...payload.clips[1], fingerprint: 'nope' }] })).toThrow('fingerprint')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], opacity: 1.5 }] })).toThrow('opacity')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], opacity: 'dim' as unknown as number }] })).toThrow('opacity')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [...payload.clips, { ...textClip, assetRef: 'asset-d' }] })).toThrow('keys')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [...payload.clips, { ...textClip, style: { ...textClip.style, fontSizeRatio: 2 } }] })).toThrow('fontSizeRatio')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [...payload.clips, { ...textClip, style: { ...textClip.style, text: '   ' } }] })).toThrow('text content')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [...payload.clips, { ...payload.clips[0], style: textClip.style }] })).toThrow('keys')
    const fadeBase = { videoInTicks: 0, videoInMode: 'none', videoOutTicks: 0, audioInTicks: 0, audioOutTicks: 0 }
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], id: 'clip-f', fades: { ...fadeBase, videoInTicks: 500 } }, payload.clips[1], payload.clips[2]] })).toThrow('fade mode')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], id: 'clip-f', fades: { ...fadeBase, audioOutTicks: 2_500 } }, payload.clips[1], payload.clips[2]] })).toThrow('fade window')
    expect(() => parseBrowserTimelineRender({ ...payload, clips: [{ ...payload.clips[0], id: 'clip-f', fades: { ...fadeBase, extra: 1 } as unknown as typeof fadeBase }, payload.clips[1], payload.clips[2]] })).toThrow('fades')
    expect(() => buildBrowserTimelineFfmpegArgv(
      { ...payload, clips: [payload.clips[1], { ...payload.clips[1], id: 'clip-b2', mediaKind: 'image' }] },
      { inputs, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } },
    )).toThrow('inconsistent')
  })

  it('rejects injected filters, unsafe paths and blocked budgets', () => {
    expect(() => buildBrowserTimelineFfmpegArgv(payload, { inputs, workerFs: true, memoryBudgetBytes: 1e9, postAudioFilters: ['volume=1;aevalsrc=evil'], output: { filename: 'x.mp4', args: [] } })).toThrow('injected')
    const bad = new Map(inputs); bad.set('asset-a', { ...bad.get('asset-a')!, path: 'bad\npath' })
    expect(() => buildBrowserTimelineFfmpegArgv(payload, { inputs: bad, workerFs: true, memoryBudgetBytes: 1e9, output: { filename: 'x.mp4', args: [] } })).toThrow('path')
    expect(() => buildBrowserTimelineFfmpegArgv(payload, { inputs, workerFs: true, memoryBudgetBytes: 1, output: { filename: 'x.mp4', args: [] } })).toThrow('memory')
    const huge = new Map(inputs); huge.set('asset-a', { ...huge.get('asset-a')!, sizeBytes: 129 * 1024 * 1024 })
    expect(estimateBrowserTimelineResources(parseBrowserTimelineRender(payload), huge, { workerFs: false, memoryBudgetBytes: 1e9 }).risk).toBe('blocked')
  })
})
