import {writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require the Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,it,expect,vi} from 'vitest'
import {evaluateNestedFrame} from '$lib/api'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import {nestedEffectPreview} from '$lib/state/nestedEffectPreview.svelte'
import {authState} from '$lib/state/auth.svelte'
import {effectScalarProject,effectColorScalarProject,effectLumetriScalarProject} from '$lib/composition/fixtures/nativeEffectScalarProject'
import {nativeScalarChannels} from '$lib/composition/nativeScalarTimeline'
import Timeline from './NestedScalarTimeline.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(element=>element.textContent===text)!
beforeEach(()=>{closeNestedComposition();authState.token='effect-session';nestedEffectPreview.value=null;vi.mocked(evaluateNestedFrame).mockReset();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document=effectScalarProject();nestedComposition.projectId='effect-project';nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1500000;nestedComposition.revision=3;nestedComposition.renderedOutputs={root:'old-render'}})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();authState.token=null;nestedEffectPreview.value=null;document.body.innerHTML=''})
async function open(){component=mount(Timeline,{target:document.body});await tick();button('Выбрать все ключи').click();await tick()}
it('authors effect values through the mounted native timeline as one preview/commit/Undo and retains sibling effects',async()=>{
 const before=JSON.stringify(nestedComposition.document);await open();expect(document.body.textContent).toContain('Выбрано: 3');expect(document.querySelector('[aria-label="shape concat_exposure #1 stops ключ 250"]')).not.toBeNull()
 const number=document.querySelector<HTMLInputElement>('[aria-label="Число для значений нативных ключей"]')!;number.value='-1';number.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Предпросмотр значений ключей').click();await tick();expect(nestedEffectPreview.value).not.toBeNull();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(evaluateNestedFrame).not.toHaveBeenCalled();const preview=JSON.stringify(nestedEffectPreview.value!.project)
 button('Применить значения ключей').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(nestedEffectPreview.value).toBeNull();expect(nestedComposition.renderedOutputs).toEqual({});expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;expect('effects' in clip&&clip.effects?.[1]).toEqual({kind:'concat_exposure',parameters:{stops:0},animation:{mode:'constant',value:0}})
 expect(nativeScalarChannels(nestedComposition.document!,'root')[0]!.value.track.sampleClock).toEqual({rateNum:2,rateDen:1,offsetNum:250000})
 const oracle=process.env.NATIVE_EFFECT_SCALAR_UI_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(document.body.textContent).toContain('Выбрано: 3');redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})
it('cancels a pending effect-key acceptance after the effect stack is replaced',async()=>{
 await open();let finish!:(value:{entries:[]})=>void;vi.mocked(evaluateNestedFrame).mockImplementation(()=>new Promise(resolve=>{finish=resolve}))
 const element=document.querySelector<HTMLButtonElement>('[aria-label="shape concat_exposure #1 stops ключ 250"]')!;element.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await vi.waitFor(()=>expect(finish).toBeDefined())
 const project=JSON.parse(JSON.stringify(nestedComposition.document!));Object.assign(project.compositions.root!.content.tracks[0]!.clips[0]!,{effects:[]});nestedComposition.document=project;await tick();finish({entries:[]});await vi.waitFor(()=>expect(nestedHistory.busy).toBe(false));expect(nestedComposition.document).toEqual(project);expect(nestedHistory.past).toHaveLength(0);expect(nestedEffectPreview.value).toBeNull()
})

it('authors a qualified color component through the mounted graph without changing amount or alpha',async()=>{
 nestedComposition.document=effectColorScalarProject();const before=JSON.stringify(nestedComposition.document);await open();expect(document.querySelector('[aria-label="shape color_tint #1 colorAnimation.whiteR ключ 250"]')).not.toBeNull()
 const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick();expect(document.querySelector('[aria-label="Точка графика shape color_tint #1 colorAnimation.whiteR 250"]')).not.toBeNull()
 const number=document.querySelector<HTMLInputElement>('[aria-label="Число для значений нативных ключей"]')!;number.value='-0.25';number.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Предпросмотр значений ключей').click();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);const preview=JSON.stringify(nestedEffectPreview.value!.project)
 button('Применить значения ключей').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(nativeScalarChannels(nestedComposition.document!,'root')[0]!.value.track.keyframes.map(key=>key.value)).toEqual([0,.25,.5]);expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!;expect('effects' in clip&&clip.effects?.[0]).toMatchObject({animation:{mode:'constant',value:100},colorAnimation:{whiteA:{mode:'constant',value:1}}})
 const oracle=process.env.NATIVE_EFFECT_COLOR_SCALAR_UI_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(document.body.textContent).toContain('Выбрано: 3');redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})

it('uses a fractional keyboard value step for effect color components',async()=>{
 nestedComposition.document=effectColorScalarProject();await open();const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick()
 document.querySelector<SVGElement>('[aria-label="Точка графика shape color_tint #1 colorAnimation.whiteR 250"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));expect(nativeScalarChannels(nestedComposition.document!,'root')[0]!.value.track.keyframes.map(key=>key.value)).toEqual([.26,.51,.76])
})

it('authors nested Lumetri exposure keys through the shared native graph and qualifies a fresh raster trace',async()=>{
 nestedComposition.document=effectLumetriScalarProject();const before=JSON.stringify(nestedComposition.document);await open();expect(document.querySelector('[aria-label="shape lumetri #1 lumetri.basic.exposure ключ 250"]')).not.toBeNull()
 const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick();expect(document.querySelector('[aria-label="Точка графика shape lumetri #1 lumetri.basic.exposure 250"]')).not.toBeNull()
 const number=document.querySelector<HTMLInputElement>('[aria-label="Число для значений нативных ключей"]')!;number.value='-1';number.dispatchEvent(new Event('input',{bubbles:true}));await tick();button('Предпросмотр значений ключей').click();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);const preview=JSON.stringify(nestedEffectPreview.value!.project)
 button('Применить значения ключей').click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick();expect(JSON.stringify(nestedComposition.document)).toBe(preview);expect(nativeScalarChannels(nestedComposition.document!,'root')[0]!.value.track.keyframes.map(key=>key.value)).toEqual([-3,-2,-1]);expect(evaluateNestedFrame).toHaveBeenCalledOnce()
 const oracle=process.env.NATIVE_LUMETRI_SCALAR_UI_ORACLE
 if(oracle)writeFileSync(oracle,JSON.stringify({schemaVersion:1,before:JSON.parse(before).compositions.root.content,after:nestedComposition.document!.compositions.root!.content,timeUs:nestedComposition.timeUs}))
 undoNestedComposition();await tick();expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(document.body.textContent).toContain('Выбрано: 3');redoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(preview)
})

it('nudges a Lumetri wheel in fractional units without touching another section',async()=>{
 const project=effectLumetriScalarProject(),clip=project.compositions.root!.content.tracks[0]!.clips[0]!;Object.assign(clip,{effects:[{kind:'lumetri',parameters:{animation:{grade:{shadowsX:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:1000000,value:.2}]}}}}}}]});nestedComposition.document=project;await open()
 const details=[...document.querySelectorAll('details')].find(element=>element.querySelector('summary')?.textContent==='График значений и кривых')!;details.open=true;details.dispatchEvent(new Event('toggle'));await tick();document.querySelector('[aria-label="Точка графика shape lumetri #1 lumetri.grade.shadowsX 0"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowUp',bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));const values=nativeScalarChannels(nestedComposition.document!,'root')[0]!.value.track.keyframes.map(key=>key.value);expect(values[0]).toBeCloseTo(.01);expect(values[1]).toBeCloseTo(.21)
})
