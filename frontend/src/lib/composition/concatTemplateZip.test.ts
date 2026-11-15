import { TextReader, Uint8ArrayWriter, ZipWriter } from '@zip.js/zip.js'
import { expect, it } from 'vitest'
import { readConcatTemplateCaptions } from './concatTemplateCaptions'
import { readConcatTemplateFolder } from './concatTemplateFolder'
import { readConcatTemplateZip } from './concatTemplateZip'

it('rejects traversal entries before handing any ZIP content to the importer', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  await writer.add('template.json', new TextReader('{}'), { level: 0 })
  await writer.add('assets/../private.txt', new TextReader('outside'), { level: 0 })
  const file = new File([new Uint8Array(await writer.close())], 'template.zip')
  await expect(readConcatTemplateZip(file, new AbortController().signal))
    .rejects.toThrow(/Некорректный путь|Unsafe filename/)
})

it('opens an archived template directory without mixing in sibling assets', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  const video = { width: 320, height: 180, rateNum: 24, rateDen: 1, colorSpace: 'sdr' }
  const first = { id: 'cue', kind: 'text', start: 1, duration: 1, name: 'Old name', text: { content: 'Text', shadow: true } }
  const second = { id: 'cue2', kind: 'text', start: 2, duration: 1, text: { content: 'Other', shadow: true } }
  const source = {
    media: [{ id: 'design', kind: 'image', path: 'assets/design.png' }],
    timelines: [{ id: 'TL1', video, clips: [first, second] }],
  }
  const manifest = JSON.stringify(source)
  await writer.add('Design/template.json', new TextReader(manifest))
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(manifest))
  const manifestSha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('')
  const constant = { mode: 'constant', value: 1 }
  const staticClip = {
    id: 'cue', text: 'Text', timelineStartTick: 1000000, timelineEndTick: 2000000, style: { align: 'center', shadowX: 4, shadow: { red: 0.1, green: 0.2, blue: 0.3, alpha: 0.5 } },
    opacity: constant, transform: { anchorX: 0.5, anchorY: 0.5, x: constant, y: constant, scaleX: constant, scaleY: constant, rotationDegrees: constant },
  }
  const clips = [
    ['TL1', staticClip],
    ['TL1', { id: 'cue2', text: 'Other', timelineStartTick: 2000000, timelineEndTick: 3000000 }],
  ]
  const captured = clips.map((entry, index) => [...entry, { clip: source.timelines[0]!.clips[index], video, uniqueClipId: true }])
  await writer.add('Design/video-kadr-caption-clips.json', new TextReader(JSON.stringify({ manifestSha256, clips: captured })))
  await writer.add('Design/assets/design.png', new TextReader('bundled'))
  await writer.add('Design/poster.jpg', new TextReader('poster'))
  const subtitles = [{ id: 'cue', name: 'cue.srt', text: '1\n00:00:00,000 --> 00:00:01,000\nТекст' }]
  await writer.add('Design/video-kadr-subtitles.json', new TextReader(JSON.stringify(subtitles)))
  await writer.add('assets/design.png', new TextReader('unrelated'))
  const zip = new File([new Uint8Array(await writer.close())], 'Design.zip')
  const selected = await readConcatTemplateZip(zip, new AbortController().signal)
  const folder = await readConcatTemplateFolder(selected)
  expect(await folder.design[0]?.file.text()).toBe('bundled')
  expect(await folder.poster?.text()).toBe('poster')
  expect(folder.subtitleDocuments).toEqual(subtitles)
  expect(folder.captionClips).toEqual(clips)
  expect(selected).toHaveLength(5)
  const sidecar = selected.find(file => file.name === 'video-kadr-caption-clips.json')!
  await expect(readConcatTemplateCaptions(sidecar, new File([manifest + ' '], 'template.json'))).resolves.toEqual(clips)
  const retimed = { ...source, video, timelines: [{ id: 'TL1', video: { rateNum: 30_000, rateDen: 1_001 }, clips: [first, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(retimed)], 'template.json'))).resolves.toEqual(clips)
  const changed = { ...source, timelines: [{
    id: 'TL1', video: { width: 320, height: 180, rateNum: 24, rateDen: 1 }, clips: [
      { ...first, text: { ...first.text, lineHeight: 1.5 } },
      { ...second, sourceStart: 0, mediaId: '', fadeIn: 0, filters: [] },
    ],
  }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(changed)], 'template.json')))
    .resolves.toEqual([clips[1]])
  const moved = { ...first, start: 5.000001, name: 'New name', text: { ...first.text, content: 'Edited' } }
  const movedTimeline = { id: 'TL2', video, clips: [moved] }
  const remainingTimeline = { id: 'TL1', video, clips: [second] }
  const relocated = new File([JSON.stringify({ ...source, timelines: [remainingTimeline, movedTimeline] })], 'template.json')
  await expect(readConcatTemplateCaptions(sidecar, relocated)).resolves.toEqual([
    ['TL2', { ...staticClip, text: 'Edited', timelineStartTick: 5000001, timelineEndTick: 6000001 }],
    clips[1],
  ])
  const ambiguous = new File([JSON.stringify({ ...source, timelines: [
    remainingTimeline, movedTimeline, { ...movedTimeline, id: 'TL3' },
  ] })], 'template.json')
  await expect(readConcatTemplateCaptions(sidecar, ambiguous)).resolves.toEqual([clips[1]])
  const withEdit = { ...source, timelines: [{ id: 'TL1', video, clips: [moved, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withEdit)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, text: 'Edited', timelineStartTick: 5000001, timelineEndTick: 6000001 }],
      clips[1],
    ])
  withEdit.timelines[0]!.clips[0] = { ...moved, start: 5.500001, duration: 0.5 }
  const trimmedTitle = { ...withEdit.timelines[0]!.clips[0], sourceStart: 0.5 }
  const trimmed = { ...withEdit, timelines: [{ id: 'TL1', video, clips: [trimmedTitle, second] }] }
  const trimmedFile = new File([JSON.stringify(trimmed)], 'template.json')
  await expect(readConcatTemplateCaptions(sidecar, trimmedFile)).resolves.toEqual([
    ['TL1', { ...staticClip, text: 'Edited', timelineStartTick: 5500001, timelineEndTick: 6000001 }],
    clips[1],
  ])
  const animated = { ...staticClip, opacity: { mode: 'keyframes' } }
  const animatedSidecar = new File([JSON.stringify({ manifestSha256, clips: [
    ['TL1', animated, { clip: first, video }], captured[1],
  ] })], 'video-kadr-caption-clips.json')
  await expect(readConcatTemplateCaptions(animatedSidecar, trimmedFile)).resolves.toEqual([clips[1]])
  const recolored = { ...first, text: { ...first.text, color: '#4080ff80', strokeColor: '#112233', background: '' } }
  const changedColor = { ...source, timelines: [{ id: 'TL1', video, clips: [recolored, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(changedColor)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, style: { ...staticClip.style,
        color: { red: 64 / 255, green: 128 / 255, blue: 1, alpha: 128 / 255 },
        stroke: { red: 17 / 255, green: 34 / 255, blue: 51 / 255, alpha: 1 },
        background: { red: 0, green: 0, blue: 0, alpha: 0 },
      } }],
      clips[1],
    ])
  const styled = { ...first, text: { ...first.text, fontSize: 0.1, fontFamily: 'Arial', fontWeight: 700, italic: true, strokeWidth: 2 / 180, align: 'right' } }
  const withStyle = { ...source, timelines: [{ id: 'TL1', video, clips: [styled, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withStyle)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, style: { ...staticClip.style, fontSize: 18, fontFamily: 'Arial', bold: true, italic: true, strokeWidth: 2, align: 'right' } }],
      clips[1],
    ])
  styled.text.fontWeight = 550
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withStyle)], 'template.json')))
    .resolves.toEqual([clips[1]])
  const resetBaseline = { ...first, text: { ...first.text, fontSize: 0.1, fontWeight: 400, italic: true,
    color: '#4080ff', strokeWidth: 2 / 180, strokeColor: '#112233', background: '#112233', align: 'right', maxWidth: 0.5 } }
  const resetSidecar = new File([JSON.stringify({ manifestSha256, clips: [
    ['TL1', { ...staticClip, textBoxWidth: 160 }, { clip: resetBaseline, video }], captured[1],
  ] })], 'video-kadr-caption-clips.json')
  await expect(readConcatTemplateCaptions(resetSidecar, new File([manifest + ' '], 'template.json'))).resolves.toEqual([
    ['TL1', { ...staticClip, style: { ...staticClip.style, fontSize: 0.09 * 180, bold: true, italic: false, strokeWidth: 0,
      color: { red: 1, green: 1, blue: 1, alpha: 1 }, stroke: { red: 0, green: 0, blue: 0, alpha: 1 },
      background: { red: 0, green: 0, blue: 0, alpha: 0 },
    } }],
    clips[1],
  ])
  const pivoted = { ...first, keys: [{ property: 'rotation', at: 0, value: 30 }],
    text: { ...first.text, align: 'center' } }
  const pivotSidecar = new File([JSON.stringify({ manifestSha256, clips: [
    ['TL1', staticClip, { clip: pivoted, video }], captured[1],
  ] })], 'video-kadr-caption-clips.json')
  const withPivot = { ...source, timelines: [{ id: 'TL1', video, clips: [
    { ...pivoted, text: { ...pivoted.text, align: 'left' } }, second,
  ] }] }
  await expect(readConcatTemplateCaptions(pivotSidecar, new File([JSON.stringify(withPivot)], 'template.json')))
    .resolves.toEqual([clips[1]])
  const narrowed = { ...first, text: { ...first.text, maxWidth: 0.625, shadow: false } }
  const withWidth = { ...source, timelines: [{ id: 'TL1', video, clips: [narrowed, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withWidth)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, textBoxWidth: 200, style: { ...staticClip.style, shadow: { ...staticClip.style.shadow, alpha: 0 } } }],
      clips[1],
    ])
  narrowed.text.maxWidth = 0
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withWidth)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, style: { ...staticClip.style, shadow: { ...staticClip.style.shadow, alpha: 0 } } }],
      clips[1],
    ])
  const placed = { ...first, offsetX: 0.25, offsetY: -0.125 }
  const withPlacement = { ...source, timelines: [{ id: 'TL1', video, clips: [placed, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withPlacement)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, transform: { ...staticClip.transform,
        x: { mode: 'constant', value: 81 }, y: { mode: 'constant', value: -21.5 },
      } }],
      clips[1],
    ])
  const transformed = { ...placed, scale: 2, stretchX: 1.5, stretchY: 0.5, rotation: 30, blend: 'add', flipH: true, flipV: true }
  const withTransform = { ...source, timelines: [{ id: 'TL1', video, clips: [transformed, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withTransform)], 'template.json')))
    .resolves.toEqual([
      ['TL1', { ...staticClip, blendMode: 'addition', transform: { ...staticClip.transform,
        x: { mode: 'constant', value: 81 }, y: { mode: 'constant', value: -21.5 },
        scaleX: { mode: 'constant', value: 3 }, scaleY: constant,
        rotationDegrees: { mode: 'constant', value: 31 },
        concatPreparation: { crop: [0, 0, 0, 0], canvas: [320, 180], flipH: true, flipV: true },
      } }],
      clips[1],
    ])
  const reflectedSidecar = new File([JSON.stringify({ manifestSha256, clips: [
    ['TL1', { ...staticClip, transform: { ...staticClip.transform,
      concatPreparation: { crop: [0, 0, 0, 0], canvas: [320, 180], flipH: true, flipV: true },
    } }, { clip: { ...first, flipH: true, flipV: true, crop: { left: 0, top: 0, right: 0, bottom: 0 } }, video }],
    captured[1],
  ] })], 'video-kadr-caption-clips.json')
  const unreflected = { ...source, timelines: [{ id: 'TL1', video, clips: [
    { ...first, text: { ...first.text, content: 'Unreflected' } }, second,
  ] }] }
  await expect(readConcatTemplateCaptions(reflectedSidecar, new File([JSON.stringify(unreflected)], 'template.json')))
    .resolves.toEqual([['TL1', { ...staticClip, text: 'Unreflected' }], clips[1]])
  const cropped = { ...source, timelines: [{ id: 'TL1', video, clips: [{ ...first, crop: { left: 0.25 } }, second] }] }
  await expect(readConcatTemplateCaptions(reflectedSidecar, new File([JSON.stringify(cropped)], 'template.json')))
    .resolves.toEqual([clips[1]])
  const faded = { ...first, opacity: 0.5, text: { ...first.text, opacity: 0.25 } }
  const withOpacity = { ...source, timelines: [{ id: 'TL1', video, clips: [faded, second] }] }
  const fadedFile = new File([JSON.stringify(withOpacity)], 'template.json')
  await expect(readConcatTemplateCaptions(sidecar, fadedFile)).resolves.toEqual([
    ['TL1', { ...staticClip, opacity: { mode: 'constant', value: 0.125 } }],
    clips[1],
  ])
  await expect(readConcatTemplateCaptions(animatedSidecar, fadedFile)).resolves.toEqual([clips[1]])
  const keyed = { ...first, keys: [{ property: 'opacity', at: 0.5, value: 0.4 }] }
  const withKeys = { ...source, timelines: [{ id: 'TL1', video, clips: [keyed, second] }] }
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(withKeys)], 'template.json')))
    .resolves.toEqual([['TL1', staticClip, { offsetX: 0, offsetY: 0 }], clips[1]])
  const keySidecar = new File([JSON.stringify({ manifestSha256, clips: [
    ['TL1', { ...staticClip, opacity: { mode: 'keyframes' } }, { clip: keyed, video }], captured[1],
  ] })], 'video-kadr-caption-clips.json')
  await expect(readConcatTemplateCaptions(keySidecar, new File([manifest + ' '], 'template.json')))
    .resolves.toEqual([['TL1', { ...staticClip, opacity: { mode: 'keyframes' } }, { offsetX: 0, offsetY: 0 }], clips[1]])
  changed.timelines[0]!.video.width = 640
  await expect(readConcatTemplateCaptions(sidecar, new File([JSON.stringify(changed)], 'template.json')))
    .resolves.toBeUndefined()
})

it('rejects a ZIP with two candidate template directories', async () => {
  const writer = new ZipWriter(new Uint8ArrayWriter(), { useWebWorkers: false })
  await writer.add('One/template.json', new TextReader('{}'))
  await writer.add('Two/template.json', new TextReader('{}'))
  const zip = new File([new Uint8Array(await writer.close())], 'Templates.zip')
  await expect(readConcatTemplateZip(zip, new AbortController().signal)).rejects.toThrow('один шаблон')
})
