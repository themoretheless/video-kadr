// @ts-expect-error The mounted component needs Svelte's client runtime.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import { nestedDurations, prepareCaptionFileImport, writeCaptionFile } from '$lib/api'
import { createComposition } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { dropSubtitle } from '$lib/subtitles/timelineDrop'
import NestedCaptionExport from './NestedCaptionExport.svelte'

vi.mock('$lib/api', () => ({
 nestedDurations: vi.fn(async () => ({ root: 4_000_000 })),
 writeCaptionFile: vi.fn(),
 prepareCaptionFileImport: vi.fn(),
}))

it.each(['picker', 'drop'])('imports an exact-clock caption track at the cursor through %s with one Undo', async method => {
 closeNestedComposition()
 vi.mocked(nestedDurations).mockClear()
 const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
 nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } }
 nestedComposition.timeUs = 1000000
 vi.mocked(prepareCaptionFileImport).mockResolvedValue({
  name: 'Broadcast', format: 'CEA-708', language: 'fr', enabled: true, locked: false, syncLock: true,
  backgroundColor: [0, 0, 0, 191],
  document: { ticksPerSecond: '254016000000', blocks: ['NOTE retained'], warnings: ['Source warning'],
   cues: [{ startTick: '1', endTick: '254016000003', text: 'Dialogue', speaker: 'Voice', id: 'external', settings: 'line:80%' }],
  },
 })
 const target = document.createElement('div')
 document.body.append(target)
 const component = mount(NestedCaptionExport, { target })
 try {
  await tick()
  const atPlayhead = target.querySelector<HTMLInputElement>('input[type="checkbox"]')!
  atPlayhead.checked = true
  atPlayhead.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  nestedComposition.timeUs = 1000000
  const file = new File(['mock MCC'], 'broadcast.MCC')
  if (method === 'picker') {
   const input = target.querySelector<HTMLInputElement>('[aria-label="Импорт на дорожку субтитров"]')!
   Object.defineProperty(input, 'files', { value: [file] })
   input.dispatchEvent(new Event('change', { bubbles: true }))
  } else {
   const zone = target.querySelector('[aria-label="Область импорта на дорожку субтитров"]')!
   const drop = (files: File[]) => {
    const event = new Event('drop', { bubbles: true, cancelable: true })
    Object.defineProperty(event, 'dataTransfer', { value: { files } })
    expect(zone.dispatchEvent(event)).toBe(false)
   }
   drop([new File([], 'video.mov')])
   await tick()
   expect(target.querySelector('[role="alert"]')?.textContent).toContain('DFXP')
   drop([file, file])
   await tick()
   expect(target.querySelector('[role="alert"]')?.textContent).toContain('один файл')
   expect(prepareCaptionFileImport).not.toHaveBeenCalled()
   drop([file])
  }
  await vi.waitFor(() => expect(target.textContent).toContain('Source warning'))
  expect(prepareCaptionFileImport).toHaveBeenCalledExactlyOnceWith(btoa('mock MCC'), 'broadcast.MCC', { num: 24000, den: 1000 })
  expect(nestedDurations).toHaveBeenCalledOnce()
  expect(nestedHistory.past).toHaveLength(1)
  expect(nestedComposition.document?.compositions.root?.content.captionTracks).toMatchObject([{
   name: 'Broadcast', format: 'CEA-708', language: 'fr', syncLock: true,
   vttBlocks: ['NOTE retained'], style: { backgroundColor: '#000000bf' },
   clips: [{ timelineStartTick: '254016000001', timelineEndTick: '508032000003',
    text: 'Dialogue', speaker: 'Voice', cueId: 'external', settings: 'line:80%' }],
  }])
  undoNestedComposition()
  expect(nestedComposition.document?.compositions.root?.content.captionTracks).toBeUndefined()
 } finally {
  await unmount(component)
  target.remove()
  vi.mocked(prepareCaptionFileImport).mockReset()
  vi.mocked(nestedDurations).mockClear()
  closeNestedComposition()
 }
})

it('exports a selected native caption track from the active timeline with its exact clock and metadata', async () => {
 closeNestedComposition()
 const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
 const root = { content, instances: [] }
 nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: { root, edited: {
  ...root, content: { ...content, canvas: { ...content.canvas, fpsMilli: 29970, fpsRate: { numerator: 30000, denominator: 1001 } },
   captionTracks: [
    { id: 'one', name: 'First', clips: [] },
    { id: 'two', name: 'Dialogue', locked: true, vttBlocks: ['NOTE preserved'], clips: [{
     id: 'native', cueId: 'original', timelineStartTick: '254016000001', timelineEndTick: '508032000003',
     text: '<i>Dialogue</i>', speaker: 'Voice', settings: 'line:90%',
    }] },
   ],
  },
 } } }
 nestedComposition.activeCompositionId = 'edited'
 vi.mocked(writeCaptionFile).mockResolvedValue({ extension: 'vtt', dataBase64: btoa('WEBVTT\n') })
 const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:caption')
 const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {})
 const target = document.createElement('div')
 const component = mount(NestedCaptionExport, { target })
 try {
  await tick()
  const track = target.querySelector<HTMLSelectElement>('[aria-label="Дорожка для экспорта"]')!
  track.value = 'two'
  track.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  const currentFormat = target.querySelector<HTMLSelectElement>('[aria-label="Формат экспорта субтитров"]')!
  currentFormat.value = 'vtt'
  currentFormat.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  expect([...currentFormat.options].map(option => option.value)).toEqual(['srt','vtt','scc','mcc','stl','ttml','dfxp'])
  const exportButton = [...target.querySelectorAll('button')].find(button => button.textContent === 'Экспортировать дорожку субтитров')!
  exportButton.click()
  await vi.waitFor(() => expect(target.querySelector('a[download]')).not.toBeNull())
  expect(writeCaptionFile).toHaveBeenCalledExactlyOnceWith({
   ticksPerSecond: '254016000000', blocks: ['NOTE preserved'], cues: [{
    startTick: '254016000001', endTick: '508032000003', text: '<i>Dialogue</i>', speaker: 'Voice', id: 'original', settings: 'line:90%',
   }],
  }, 'vtt', { dropFrame: true, rate: { num: 30000, den: 1001 } })
  expect(target.querySelector<HTMLAnchorElement>('a[download]')?.download).toBe('Dialogue.vtt')
  nestedComposition.timeUs = 1_500_001
  const offset = [...target.querySelectorAll('label')].find(label => label.textContent?.trim() === 'Экспортировать от курсора')!.querySelector('input')!
  offset.checked = true
  offset.dispatchEvent(new Event('change', { bubbles: true }))
  await tick()
  exportButton.click()
  await vi.waitFor(() => expect(writeCaptionFile).toHaveBeenCalledTimes(2))
  expect(vi.mocked(writeCaptionFile).mock.calls[1]?.[0]).toEqual(vi.mocked(writeCaptionFile).mock.calls[0]?.[0])
  expect(vi.mocked(writeCaptionFile).mock.calls[1]?.[2]).toEqual({
   offsetTick: '381024254016', dropFrame: true, rate: { num: 30000, den: 1001 },
  })
  await vi.waitFor(() => expect(target.querySelector('a[download]')).not.toBeNull())
  nestedComposition.activeCompositionId = 'root'
  await tick()
  expect(revoke).toHaveBeenCalledWith('blob:caption')
 } finally {
  await unmount(component)
  create.mockRestore()
  revoke.mockRestore()
  vi.mocked(writeCaptionFile).mockReset()
  closeNestedComposition()
 }
})

it('validates dropped files and imports subtitles at the cursor in one undo step', async () => {
 closeNestedComposition()
 const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
 const project = { schemaVersion: 1 as const, root: 'root', compositions: { root: { content, instances: [] } } }
 nestedComposition.document = project
 nestedComposition.timeUs = 2_000_000
 const target = document.createElement('div')
 document.body.append(target)
 let component = mount(NestedCaptionExport, { target })
 try {
  await tick()
  target.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click()
  await tick()
  const zone = target.querySelector('[aria-label="Область импорта субтитров"]')!
  const drop = (files: File[]) => {
   const event = new Event('drop', { bubbles: true, cancelable: true })
   Object.defineProperty(event, 'dataTransfer', { value: { files } })
   expect(zone.dispatchEvent(event)).toBe(false)
  }
  drop([new File(['ignored'], 'video.mov')])
  await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('Выберите файл SRT'))
  expect(nestedDurations).not.toHaveBeenCalled()
  zone.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click()
  await tick()
  const reuse = () => [...target.querySelectorAll('button')].find(item => item.textContent?.includes('Вставить · captions.SRT'))
  drop([new File(['invalid'], 'broken.srt')])
  await vi.waitFor(() => {
   const error = target.querySelector('[role="alert"]')?.textContent
   expect(error).toBeTruthy()
   expect(error).not.toContain('Выберите файл SRT')
  })
  expect(target.querySelector('[aria-label="Недавние файлы субтитров"]')).toBeNull()
  drop([new File(['1\n00:00:01,000 --> 00:00:02,000\nCaption'], 'captions.SRT')])
  await vi.waitFor(() => expect(reuse()).toBeDefined())
  expect(target.querySelector('[role="alert"]')).toBeNull()
  expect(nestedHistory.past).toHaveLength(1)
  const saved = JSON.stringify(nestedComposition.document)
  expect(nestedComposition.document?.compositions.root?.content.metadata?.subtitleDocuments).toMatchObject([
   { name: 'captions.SRT', text: expect.stringContaining('Caption') },
  ])
  await unmount(component)
  closeNestedComposition()
  nestedComposition.document = JSON.parse(saved)
  nestedComposition.timeUs = 2_000_000
  component = mount(NestedCaptionExport, { target })
  await tick()
  target.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click()
  await vi.waitFor(() => expect(reuse()).toBeDefined())
  expect(nestedDurations).not.toHaveBeenCalled()
  reuse()!.click()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(nestedDurations).toHaveBeenCalledOnce()
  expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
   text: 'Caption', timelineStartTick: 3_000_000, timelineEndTick: 4_000_000,
  })
  nestedComposition.timeUs = 1_000_000
  await vi.waitFor(() => expect(reuse()).toBeDefined())
  const transfer = new DataTransfer()
  const dragStart = new Event('dragstart', { bubbles: true, cancelable: true })
  Object.defineProperty(dragStart, 'dataTransfer', { value: transfer })
  reuse()!.dispatchEvent(dragStart)
  const dropEvent = new Event('drop', { cancelable: true }) as DragEvent
  Object.defineProperty(dropEvent, 'dataTransfer', { value: transfer })
  dropSubtitle(dropEvent, 5_000_000)
  expect(dropEvent.defaultPrevented).toBe(true)
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(2))
  expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[1]).toMatchObject({
   text: 'Caption', timelineStartTick: 6_000_000, timelineEndTick: 7_000_000,
  })
  target.querySelector<HTMLButtonElement>('[aria-label="Убрать файл captions.SRT из списка"]')!.click()
  await vi.waitFor(() => expect(reuse()).toBeUndefined())
  expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips).toHaveLength(2)
  undoNestedComposition()
  undoNestedComposition()
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(JSON.parse(saved))
  const quickText = '1\n00:00:00,000 --> 00:00:01,000\nQuick import'
  const reopenedZone = target.querySelector('[aria-label="Область импорта субтитров"]')!
  function quickImport() {
   const event = new Event('drop', { bubbles: true, cancelable: true })
   Object.defineProperty(event, 'dataTransfer', { value: { files: [new File([quickText], 'quick.srt')] } })
   reopenedZone.dispatchEvent(event)
  }
  vi.mocked(nestedDurations).mockRejectedValueOnce(new Error('Server rejected the edit'))
  quickImport()
  await vi.waitFor(() => expect(target.querySelector('[role="alert"]')?.textContent).toContain('Server rejected'))
  expect(nestedComposition.document).toEqual(JSON.parse(saved))
  expect(nestedHistory.past).toHaveLength(0)
  quickImport()
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(nestedComposition.document?.compositions.root?.content.metadata?.subtitleDocuments).toHaveLength(2)
  expect(nestedComposition.document?.compositions.root?.content.metadata?.subtitleDocuments?.[1]).toMatchObject({
   name: 'quick.srt', text: quickText,
  })
  expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({ text: 'Quick import' })
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(JSON.parse(saved))

 } finally {
  await unmount(component)
  target.remove()
  closeNestedComposition()
 }
})
