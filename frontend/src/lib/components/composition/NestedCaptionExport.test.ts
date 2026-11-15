// @ts-expect-error The mounted component needs Svelte's client runtime.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import { nestedDurations } from '$lib/api'
import { createComposition } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
import { dropSubtitle } from '$lib/subtitles/timelineDrop'
import NestedCaptionExport from './NestedCaptionExport.svelte'

vi.mock('$lib/api', () => ({ nestedDurations: vi.fn(async () => ({ root: 4_000_000 })) }))

it('validates dropped files and imports subtitles at the cursor in one undo step', async () => {
 closeNestedComposition()
 const content = buildCompositionWireDocument(createComposition({ width: 320, height: 180, fps: 24, backgroundColor: '#000000' }))
 const project = { schemaVersion: 1 as const, root: 'root', compositions: { root: { content, instances: [] } } }
 nestedComposition.document = project
 nestedComposition.timeUs = 2_000_000
 const target = document.createElement('div')
 document.body.append(target)
 const component = mount(NestedCaptionExport, { target })
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
  expect(nestedHistory.past).toHaveLength(0)
  expect(nestedComposition.document).toEqual(project)
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
  await tick()
  expect(reuse()).toBeUndefined()
  expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips).toHaveLength(2)
  undoNestedComposition()
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(project)
 } finally {
  await unmount(component)
  target.remove()
  closeNestedComposition()
 }
})
