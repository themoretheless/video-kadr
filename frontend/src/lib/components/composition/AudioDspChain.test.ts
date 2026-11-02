import {writeFileSync} from 'node:fs'
// @ts-expect-error Client entry needed for DOM tests.
import { mount, tick, unmount } from '../../../../node_modules/svelte/src/index-client.js'
import { describe, expect, it, vi } from 'vitest'
import AudioDspChain from './AudioDspChain.svelte'
const getCatalog = vi.hoisted(() => vi.fn())
vi.mock('$lib/api', async importOriginal => ({ ...(await importOriginal<typeof import('$lib/api')>()), getAudioDspCatalog: getCatalog }))
describe('native DSP chain controls', () => {
  it('loads native parameter ranges, edits gain, bypasses and removes with cancellation', async () => {
    getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: 'amplify', nativeExport: true, name: 'Amplify', params: [{ id: 'gain', name: 'Gain', min: -96, max: 24, default: 0, unit: 'decibels', choices: [] }] }] })
    const target = document.createElement('div'); document.body.append(target)
    const onchange = vi.fn()
    const component = mount(AudioDspChain, { target, props: { effects: [{ id: 'amplify' }], localTick: 500000, onchange } })
    await tick(); await Promise.resolve(); await tick()
    const gain = target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Gain"]')!
    expect(gain.min).toBe('-96'); expect(gain.max).toBe('24')
    target.querySelector<HTMLButtonElement>('[aria-label="Ключ DSP 1 Gain"]')!.click()
    expect(onchange).toHaveBeenLastCalledWith([{ id: 'amplify', params: { gain: 0 }, animation: { gain: { mode: 'keyframes', track: { timeBase: 1000000, interpolation: 'linear', keyframes: [{ tick: 500000, value: 0 }] } } } }])

    gain.value = '-6'; gain.dispatchEvent(new Event('change', { bubbles: true }))
    expect(onchange).toHaveBeenLastCalledWith([{ id: 'amplify', params: { gain: -6 } }])
    const bypass = target.querySelector<HTMLInputElement>('[aria-label="Bypass DSP 1"]')!
    bypass.checked = true; bypass.dispatchEvent(new Event('change', { bubbles: true }))
    expect(onchange).toHaveBeenLastCalledWith([{ id: 'amplify', bypass: true }])
    target.querySelector<HTMLButtonElement>('[aria-label="Удалить DSP 1"]')!.click()
    expect(onchange).toHaveBeenLastCalledWith([])
    const signal = getCatalog.mock.calls.at(-1)?.[0] as AbortSignal
    await unmount(component); target.remove()
    expect(signal.aborted).toBe(true)
  })
})

it('offers Concat Echo with independent static controls and original parameter steps', async () => {
  getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: 'concat.echo', nativeExport: true, category: 'time', name: 'Concat Echo', params: [
    { id: 'delay', name: 'Delay', min: 0.05, max: 1, default: 0.25, step: 0.05, unit: 'seconds', choices: [], animatable: false },
    { id: 'decay', name: 'Decay', min: 0.1, max: 0.9, default: 0.4, step: 0.05, unit: 'none', choices: [], animatable: false },
  ] }] })
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn()
  const component = mount(AudioDspChain, { target, props: { effects: [{ id: 'concat.echo' }], onchange } })
  await tick(); await Promise.resolve(); await tick()
  expect(target.querySelector<HTMLOptionElement>('option[value="concat.echo"]')?.textContent).toBe('Concat Echo')
  const delay = target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Delay"]')!, decay = target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Decay"]')!
  expect([delay.value, delay.min, delay.max, delay.step, decay.value, decay.min, decay.max, decay.step]).toEqual(['0.25', '0.05', '1', '0.05', '0.4', '0.1', '0.9', '0.05'])
  expect(target.querySelector<HTMLButtonElement>('[aria-label="Ключ DSP 1 Delay"]')!.disabled).toBe(true)
  expect(target.querySelector<HTMLButtonElement>('[aria-label="Ключ DSP 1 Decay"]')!.disabled).toBe(true)
  delay.value = '0.5'; delay.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: 'concat.echo', params: { delay: 0.5 } }])
  decay.value = '0.75'; decay.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: 'concat.echo', params: { decay: 0.75 } }])
  await unmount(component); target.remove()
})

for (const spec of [{ id: 'room', value: 40 }, { id: 'hall', value: 50 }, { id: 'plate', value: 40 }, { id: 'cathedral', value: 55 }, { id: 'stadium', value: 6 }]) it(`edits static Concat ${spec.id} size using its original limits and default`, async () => {
  const name = `Concat ${spec.id}`, min=spec.id==='stadium'?1:0, max=spec.id==='stadium'?10:100, step=spec.id==='stadium'?0.5:1, value=spec.id==='stadium'?7.5:75
  getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: `concat.${spec.id}`, name, nativeExport: true, category: 'reverb', params: [{ id: 'size', name: 'Size', min, max, default: spec.value, step, unit: spec.id==='stadium'?'none':'percent', choices: [], animatable: false }] }] })
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn(), component = mount(AudioDspChain, { target, props: { effects: [{ id: `concat.${spec.id}` }], onchange } })
  await tick(); await Promise.resolve(); await tick()
  const size = target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Size"]')!
  expect([size.value, size.min, size.max, size.step]).toEqual([String(spec.value), String(min), String(max), String(step)])
  expect(target.querySelector<HTMLButtonElement>('[aria-label="Ключ DSP 1 Size"]')!.disabled).toBe(true)
  size.value = String(value); size.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: `concat.${spec.id}`, params: { size: value } }])
  size.value = String(max+1); size.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenCalledTimes(1)
  await unmount(component); target.remove()
})

it('edits static Concat Distorted drive with original admission limits', async () => {
  getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: 'concat.distorted', nativeExport: true, category: 'dynamics', name: 'Concat Distorted', params: [
    { id: 'drive', name: 'Drive', min: 0, max: 100, default: 50, step: 1, unit: 'percent', choices: [], animatable: false },
  ] }] })
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn(), component = mount(AudioDspChain, { target, props: { effects: [{ id: 'concat.distorted' }], onchange } })
  await tick(); await Promise.resolve(); await tick()
  const drive = target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Drive"]')!
  expect([drive.value, drive.min, drive.max, drive.step]).toEqual(['50', '0', '100', '1'])
  expect(target.querySelector<HTMLButtonElement>('[aria-label="Ключ DSP 1 Drive"]')!.disabled).toBe(true)
  drive.value = '75'; drive.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: 'concat.distorted', params: { drive: 75 } }])
  drive.value = '101'; drive.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenCalledTimes(1)
  await unmount(component); target.remove()
})

for (const spec of [{ id: 'bass', value: 5 }, { id: 'treble', value: 4 }, { id: 'bright', value: 5 }]) it(`edits static Concat ${spec.id} gain with the original half-dB step`, async () => {
  getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: `concat.${spec.id}`, nativeExport: true, category: 'filter', name: `Concat ${spec.id}`, params: [
    { id: 'gain', name: 'Gain', min: 0, max: 12, default: spec.value, step: 0.5, unit: 'decibels', choices: [], animatable: false },
  ] }] })
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn(), component = mount(AudioDspChain, { target, props: { effects: [{ id: `concat.${spec.id}` }], onchange } })
  await tick(); await Promise.resolve(); await tick()
  const gain = target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Gain"]')!
  expect([gain.value, gain.min, gain.max, gain.step]).toEqual([String(spec.value), '0', '12', '0.5'])
  expect(target.querySelector<HTMLButtonElement>('[aria-label="Ключ DSP 1 Gain"]')!.disabled).toBe(true)
  gain.value = '7.5'; gain.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: `concat.${spec.id}`, params: { gain: 7.5 } }])
  gain.value = '12.5'; gain.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenCalledTimes(1)
  await unmount(component); target.remove()
})

for (const spec of [
  { id: 'underwater', key: 'depth', label: 'Depth', min: 1, value: 5 },
  { id: 'telephone', key: 'drive', label: 'Drive', min: 0, value: 3 },
  { id: 'megaphone', key: 'drive', label: 'Drive', min: 1, value: 5 },
  { id: 'vinyl', key: 'warmth', label: 'Warmth', min: 0, value: 5 },
  { id: 'whisper', key: 'air', label: 'Air', min: 1, value: 5 },
]) it(`edits static Concat ${spec.id} with original voice control limits`, async () => {
  getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: `concat.${spec.id}`, nativeExport: true, category: 'filter', name: `Concat ${spec.id}`, params: [
    { id: spec.key, name: spec.label, min: spec.min, max: 10, default: spec.value, step: 0.5, unit: 'none', choices: [], animatable: false },
  ] }] })
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn(), component = mount(AudioDspChain, { target, props: { effects: [{ id: `concat.${spec.id}` }], onchange } })
  await tick(); await Promise.resolve(); await tick()
  const control = target.querySelector<HTMLInputElement>(`[aria-label="DSP 1 ${spec.label}"]`)!
  expect([control.value, control.min, control.max, control.step]).toEqual([String(spec.value), String(spec.min), '10', '0.5'])
  expect(target.querySelector<HTMLButtonElement>(`[aria-label="Ключ DSP 1 ${spec.label}"]`)!.disabled).toBe(true)
  control.value = '7.5'; control.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: `concat.${spec.id}`, params: { [spec.key]: 7.5 } }])
  control.value = '10.5'; control.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenCalledTimes(1)
  await unmount(component); target.remove()
})

for (const spec of [
  { id: 'radio', key: 'noise', label: 'Noise', min: 0, max: 100, value: 30, unit: 'percent' },
  { id: 'lofi', key: 'bits', label: 'Bits', min: 4, max: 12, value: 8, unit: 'none' },
]) it(`edits static Concat ${spec.id} with original integer UI steps`, async () => {
  getCatalog.mockResolvedValue({ schemaVersion: 1, effects: [{ id: `concat.${spec.id}`, nativeExport: true, category: 'filter', name: `Concat ${spec.id}`, params: [
    { id: spec.key, name: spec.label, min: spec.min, max: spec.max, default: spec.value, step: 1, unit: spec.unit, choices: [], animatable: false },
  ] }] })
  const target = document.createElement('div'); document.body.append(target)
  const onchange = vi.fn(), component = mount(AudioDspChain, { target, props: { effects: [{ id: `concat.${spec.id}` }], onchange } })
  await tick(); await Promise.resolve(); await tick()
  const control = target.querySelector<HTMLInputElement>(`[aria-label="DSP 1 ${spec.label}"]`)!
  expect([control.value, control.min, control.max, control.step]).toEqual([String(spec.value), String(spec.min), String(spec.max), '1'])
  expect(target.querySelector<HTMLButtonElement>(`[aria-label="Ключ DSP 1 ${spec.label}"]`)!.disabled).toBe(true)
  control.value = '7'; control.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenLastCalledWith([{ id: `concat.${spec.id}`, params: { [spec.key]: 7 } }])
  control.value = String(spec.max + 1); control.dispatchEvent(new Event('change', { bubbles: true }))
  expect(onchange).toHaveBeenCalledTimes(1)
  await unmount(component); target.remove()
})

it('edits inactive Concat source keys without changing static sound parameters',async()=>{
 getCatalog.mockResolvedValue({schemaVersion:1,effects:[{id:'concat.pitch',nativeExport:true,name:'Pitch',params:[{id:'semitones',name:'Semitones',min:-12,max:12,default:0,unit:'none',choices:[],animatable:false}]}]})
 const effects=[{id:'concat.pitch',params:{semitones:0},concatSourceKeys:{semitones:[{at:0,value:-12,ease:[0,0,1,1] as [number,number,number,number]},{at:1,value:12}]}}]
 const target=document.createElement('div');document.body.append(target);const onchange=vi.fn();const component=mount(AudioDspChain,{target,props:{effects,onchange}})
 await tick();await Promise.resolve();await tick();expect(target.textContent).toContain('не меняют звук')
 const input=target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Semitones: ключ 1 значение"]')!;input.value='-7';input.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange).toHaveBeenLastCalledWith([{...effects[0],concatSourceKeys:{semitones:[{...effects[0]!.concatSourceKeys.semitones[0],value:-7},effects[0]!.concatSourceKeys.semitones[1]]}}]);expect(effects[0]!.concatSourceKeys.semitones[0]!.value).toBe(-12)
 const easing=target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Semitones: ключ 1 y1"]')!;easing.value='-0.5';easing.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange.mock.lastCall![0][0].concatSourceKeys.semitones[0].ease).toEqual([0,-0.5,1,1]);expect(onchange.mock.lastCall![0][0].params).toEqual({semitones:0});expect(onchange.mock.lastCall![0][0].animation).toBeUndefined()
 if(process.env.AUDIO_SOURCE_KEY_EDITOR_OUTPUT)writeFileSync(process.env.AUDIO_SOURCE_KEY_EDITOR_OUTPUT,JSON.stringify(onchange.mock.lastCall![0][0]))
 const invalid=target.querySelector<HTMLInputElement>('[aria-label="DSP 1 Semitones: ключ 1 положение"]')!;const before=onchange.mock.calls.length;invalid.value='2';invalid.dispatchEvent(new Event('change',{bubbles:true}));expect(onchange).toHaveBeenCalledTimes(before);await tick();expect(target.querySelector('[role="alert"]')).not.toBeNull()
 target.querySelector<HTMLButtonElement>('[aria-label="DSP 1 Semitones: удалить исходные ключи"]')!.click();expect(onchange.mock.lastCall![0][0].concatSourceKeys).toEqual({})
 await unmount(component);target.remove()
})
