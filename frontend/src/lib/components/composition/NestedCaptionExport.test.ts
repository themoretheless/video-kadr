// @ts-expect-error The mounted component needs Svelte's client runtime.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import { nestedDurations } from '$lib/api'
import { createComposition } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { closeNestedComposition, nestedComposition, nestedHistory, undoNestedComposition } from '$lib/state/nestedComposition.svelte'
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
  drop([new File(['1\n00:00:01,000 --> 00:00:02,000\nCaption'], 'captions.SRT')])
  await vi.waitFor(() => expect(nestedHistory.past).toHaveLength(1))
  expect(nestedDurations).toHaveBeenCalledOnce()
  expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({
   text: 'Caption', timelineStartTick: 3_000_000, timelineEndTick: 4_000_000,
  })
  undoNestedComposition()
  expect(nestedComposition.document).toEqual(project)
 } finally {
  await unmount(component)
  target.remove()
  closeNestedComposition()
 }
})
