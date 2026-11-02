// @ts-expect-error Tests need the Svelte client entry.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { expect, it, vi } from 'vitest'
import fixture from '../../../../../backend/tests/fixtures/concat-merge-preview.json'
import NestedSourceSpeedCurve from './NestedSourceSpeedCurve.svelte'
import { setNestedSourceSpeedCurve } from '$lib/state/nestedComposition.svelte'
import type { NativeNestedProject } from '$lib/api'
vi.mock('$lib/state/nestedComposition.svelte', () => ({ setNestedSourceSpeedCurve: vi.fn(async () => {}) }))
function project(): NativeNestedProject {
  const value = JSON.parse(JSON.stringify(fixture.nativeFirstFree[0]!.after)), node = value.compositions['new-timeline']
  node.content.canvas.fpsRate = { numerator: 25, denominator: 1 }
  const points = [{ at: 0, speed: 1 }, { at: 0.375, speed: 3 }, { at: 1, speed: 1 }]
  for (const index of [1, 3]) {
    const p = node.content.tracks[index].clips[0].placement
    p.timelineStartTick = 2000000; p.timelineDurationTick = 3000000; p.sourceInTick = 1500000; p.speed = 1.75
    p.concatRetime = { documentClock: { start: 2, duration: 3, sourceStart: 1.5, speed: 1.75, speedCurve: points }, points: points.map(point => [point.at, point.speed]), durationTicks: 3000000, rateNum: 25, rateDen: 1 }
  }
  return value
}
it('edits percentage positions and submits normalized points, while disabling invalid numeric input', async () => {
  vi.clearAllMocks()
  const target = document.createElement('div'); document.body.append(target)
  const component = mount(NestedSourceSpeedCurve, { target, props: { project: project(), compositionId: 'new-timeline', clipIds: ['auto-first'] } })
  const field = (label: string) => target.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!
  const button = (text: string) => [...target.querySelectorAll<HTMLButtonElement>('button')].find(value => value.textContent === text)!
  try {
    await tick()
    expect(field('Позиция точки скорости 2').value).toBe('37.5')
    field('Позиция точки скорости 2').value = '50'; field('Позиция точки скорости 2').dispatchEvent(new Event('input', { bubbles: true }))
    field('Скорость точки 2').value = '4'; field('Скорость точки 2').dispatchEvent(new Event('input', { bubbles: true })); await tick()
    button('Применить кривую скорости').click(); await tick()
    expect(setNestedSourceSpeedCurve).toHaveBeenCalledWith('new-timeline', ['auto-first'], [{ at: 0, speed: 1 }, { at: 0.5, speed: 4 }, { at: 1, speed: 1 }])
    field('Скорость точки 2').value = ''; field('Скорость точки 2').dispatchEvent(new Event('input', { bubbles: true })); await tick()
    expect(button('Применить кривую скорости').disabled).toBe(true)
    button('Убрать кривую скорости').click(); await tick()
    expect(setNestedSourceSpeedCurve).toHaveBeenLastCalledWith('new-timeline', ['auto-first'], null)
  } finally { await unmount(component); target.remove() }
})
it('hides unavailable source editing for a locked linked participant', async () => {
  const value = JSON.parse(JSON.stringify(project())), target = document.createElement('div')
  value.compositions['new-timeline']!.content.tracks[3] = { ...value.compositions['new-timeline']!.content.tracks[3]!, locked: true }
  const component = mount(NestedSourceSpeedCurve, { target, props: { project: value, compositionId: 'new-timeline', clipIds: ['auto-first'] } })
  try { await tick(); expect(target.querySelector('details')).toBeNull() } finally { await unmount(component) }
})

it('shows and edits retained image speed curves',async()=>{
 vi.clearAllMocks();const value=JSON.parse(JSON.stringify(project())),node=value.compositions['new-timeline'],video=node.content.tracks[1].clips[0]
 node.content.metadata.clipLinks={};node.content.tracks=[{id:'images',kind:'image',name:'Images',locked:false,hidden:false,clips:[{id:'still',sourceId:video.sourceId,timelineStartTick:2000000,durationTicks:3000000,transform:video.transform,opacity:video.opacity,blendMode:'normal',enabled:true,concatSourceTiming:{sourceStart:4,speed:2,speedCurve:[{at:0,speed:1},{at:1,speed:3}]}}]}]
 const target=document.createElement('div');document.body.append(target);const component=mount(NestedSourceSpeedCurve,{target,props:{project:value,compositionId:'new-timeline',clipIds:['still']}})
 try{await tick();expect(target.textContent).toContain('Кривая скорости');expect(target.querySelector<HTMLInputElement>('[aria-label="Скорость точки 2"]')!.value).toBe('3');const button=[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent?.includes('Применить'))!;button.click();await vi.waitFor(()=>expect(setNestedSourceSpeedCurve).toHaveBeenCalledWith('new-timeline',['still'],[{at:0,speed:1},{at:1,speed:3}]))}finally{await unmount(component);target.remove()}
})
