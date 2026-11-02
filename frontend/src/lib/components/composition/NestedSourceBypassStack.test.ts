import {readFileSync,writeFileSync} from 'node:fs'
// @ts-expect-error Mounted tests require Svelte client entry.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {afterEach,beforeEach,expect,it,vi} from 'vitest'
import {upgradeConcatEffectLinks,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {closeNestedComposition,nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition} from '$lib/state/nestedComposition.svelte'
import Inspector from './NestedEffectStack.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),evaluateNestedFrame:vi.fn(),upgradeConcatEffectLinks:vi.fn(),getAudioDspCatalog:vi.fn(async()=>({schemaVersion:1,effects:[{id:'concat.echo',nativeExport:true,name:'Echo',params:[{id:'delay',name:'Delay',min:0.05,max:1,default:0.25,unit:'seconds',choices:[]}]}]}))}))
let component:ReturnType<typeof mount>|undefined
beforeEach(()=>{closeNestedComposition();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]});nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{sources:{source:{hasAudio:true}},tracks:[{kind:'video',id:'v',name:'Video',locked:false,clips:[{id:'clip',sourceId:'source',sourceAudioEnabled:true,placement:{timelineStartTick:0,sourceInTick:0,sourceOutTick:1000000,speed:1},effects:[{kind:'concat_bypass',link:{id:'unknown.saved.package',enabled:false,params:{custom:123},keys:{custom:[{at:0,value:-7}]}}},{kind:'horizontal_flip'}]}]}]},instances:[]}}} as unknown as NativeNestedProject;nestedComposition.renderedOutputs={root:'stale'};vi.clearAllMocks()})
afterEach(async()=>{if(component)await unmount(component);component=undefined;closeNestedComposition();document.body.innerHTML=''})

it('retains disabled source data while reordering with native history',async()=>{
 component=mount(Inspector,{target:document.body});await tick();expect(document.body.textContent).toContain('Отключён: unknown.saved.package')
 document.querySelector<HTMLButtonElement>('[aria-label="clip effect 0 down"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const effects=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.effects!}
 expect(effects()[1]).toMatchObject({kind:'concat_bypass',link:{params:{custom:123},keys:{custom:[{value:-7}]}}});undoNestedComposition();expect(effects()[0]!.kind).toBe('concat_bypass');redoNestedComposition();expect(effects()[1]!.kind).toBe('concat_bypass')
})

function upgradeResponse():Awaited<ReturnType<typeof upgradeConcatEffectLinks>>{
 if(process.env.VISUAL_BYPASS_UPGRADE_INPUT)return JSON.parse(readFileSync(process.env.VISUAL_BYPASS_UPGRADE_INPUT,'utf8'))
 return {schemaVersion:1,links:[{upgraded:false,link:{id:'concat.exposure',enabled:true,params:{stops:4},keys:{stops:[{at:0,value:0},{at:1,value:4,ease:'inOut'}]}}}],nativeEffects:[{kind:'concat_exposure',parameters:{stops:4},concatKeys:[{at:0,value:0},{at:1,value:4,ease:[0.42,0,0.58,1]}]}]}
}
function knownSource(){const document=JSON.parse(JSON.stringify(nestedComposition.document));document.compositions.root.content.tracks[0].clips[0].effects[0].link={...upgradeResponse().links[0]!.link,enabled:false};nestedComposition.document=document}
const effects=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.effects!}
it('reactivates an actual converted source descriptor with keys and restores bypass through undo',async()=>{
 knownSource();const result=upgradeResponse();vi.mocked(upgradeConcatEffectLinks).mockResolvedValue(result);component=mount(Inspector,{target:document.body});await tick();document.querySelector<HTMLButtonElement>('[aria-label="clip effect 0 enable"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 expect(upgradeConcatEffectLinks).toHaveBeenCalledWith([{...result.links[0]!.link,enabled:true}],undefined,true);expect(effects()[0]).toEqual(result.nativeEffects![0]);expect(effects()[1]!.kind).toBe('horizontal_flip');expect(nestedComposition.renderedOutputs).toEqual({})
 if(process.env.VISUAL_BYPASS_REACTIVATED_OUTPUT)writeFileSync(process.env.VISUAL_BYPASS_REACTIVATED_OUTPUT,JSON.stringify(effects()[0]))
 undoNestedComposition();expect(effects()[0]!.kind).toBe('concat_bypass');expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});redoNestedComposition();expect(effects()[0]).toEqual(result.nativeEffects![0])
})
it('retains unsupported saved data and reports failure without a history entry',async()=>{
 const before=JSON.stringify(nestedComposition.document);vi.mocked(upgradeConcatEffectLinks).mockResolvedValue({schemaVersion:1,links:[{upgraded:false,link:{id:'unknown.saved.package',enabled:true}}],nativeEffects:[null]});component=mount(Inspector,{target:document.body});await tick();document.querySelector<HTMLButtonElement>('[aria-label="clip effect 0 enable"]')!.click();await vi.waitFor(()=>expect(nestedHistory.error).toContain('пока нельзя включить'));expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'stale'});expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('discards a conversion returned after project closure',async()=>{
 knownSource();let resolve!:(value:Awaited<ReturnType<typeof upgradeConcatEffectLinks>>)=>void;vi.mocked(upgradeConcatEffectLinks).mockReturnValue(new Promise(done=>resolve=done));component=mount(Inspector,{target:document.body});await tick();document.querySelector<HTMLButtonElement>('[aria-label="clip effect 0 enable"]')!.click();await vi.waitFor(()=>expect(upgradeConcatEffectLinks).toHaveBeenCalled());closeNestedComposition();resolve(upgradeResponse());await tick();await tick();expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toEqual([])
})

it('toggles active native effects through mounted controls and history',async()=>{
 component=mount(Inspector,{target:document.body});await tick()
 for(let cycle=0;cycle<3;cycle++){
  document.querySelector<HTMLButtonElement>('[aria-label="clip effect 1 enable"]')!.click()
  await vi.waitFor(()=>expect(effects()[1]).toEqual({kind:'native_bypass',effect:{kind:'horizontal_flip'}}))
  document.querySelector<HTMLButtonElement>('[aria-label="clip effect 1 enable"]')!.click()
  await vi.waitFor(()=>expect(effects()[1]).toEqual({kind:'horizontal_flip'}))
 }
 expect(upgradeConcatEffectLinks).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(6)
 undoNestedComposition();expect(effects()[1]).toEqual({kind:'native_bypass',effect:{kind:'horizontal_flip'}})
 redoNestedComposition();expect(effects()[1]).toEqual({kind:'horizontal_flip'})
})

it('rejects a native toggle on a locked track before rendering',async()=>{
 const snapshot=JSON.parse(JSON.stringify(nestedComposition.document));snapshot.compositions.root.content.tracks[0].locked=true;nestedComposition.document=snapshot
 component=mount(Inspector,{target:document.body});await tick()
 const button=document.querySelector<HTMLButtonElement>('[aria-label="clip effect 1 enable"]')!
 expect(button.disabled).toBe(true);button.click();await tick()
 expect(effects()[1]).toEqual({kind:'horizontal_flip'});expect(nestedHistory.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('does not restore a closed project when native toggle validation finishes',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof evaluateNestedFrame>>)=>void
 vi.mocked(evaluateNestedFrame).mockReturnValueOnce(new Promise(done=>resolve=done))
 component=mount(Inspector,{target:document.body});await tick();document.querySelector<HTMLButtonElement>('[aria-label="clip effect 1 enable"]')!.click()
 await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());closeNestedComposition();resolve({entries:[]});await tick();await tick()
 expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toEqual([])
})

it('edits visual source keys through mounted history and rejects authored values outside property bounds',async()=>{
 const snapshot=JSON.parse(JSON.stringify(nestedComposition.document));snapshot.compositions.root.content.tracks[0].clips[0].transform={concatAnimation:{durationTicks:1000000,keys:{opacity:[{at:0,value:0},{at:1,value:1}]},rest:{scaleX:1,scaleY:1,x:0,y:0,rotationDegrees:0,opacity:1},stretch:[1,1],canvas:[16,16]}};nestedComposition.document=snapshot
 component=mount(Inspector,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Непрозрачность: ключ 2 значение"]')!
 input.value='0.6';input.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1))
 const animation=()=>{const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.transform.concatAnimation!}
 expect(animation().keys.opacity![1]!.value).toBe(0.6);expect(nestedComposition.renderedOutputs).toEqual({})
 undoNestedComposition();expect(animation().keys.opacity![1]!.value).toBe(1);redoNestedComposition();expect(animation().keys.opacity![1]!.value).toBe(0.6)
 await tick();const invalid=document.querySelector<HTMLInputElement>('[aria-label="Непрозрачность: ключ 2 значение"]')!;invalid.value='2';invalid.dispatchEvent(new Event('change',{bubbles:true}))
 await vi.waitFor(()=>expect(nestedHistory.error).toContain('Недопустимые ключи'));expect(animation().keys.opacity![1]!.value).toBe(0.6);expect(nestedHistory.past).toHaveLength(1)
})

it.skipIf(!process.env.CONCAT_POSE_IMPORTED_INPUT)('captures source-pose edits from an actual imported project for real delivery',async()=>{
 const project=JSON.parse(readFileSync(process.env.CONCAT_POSE_IMPORTED_INPUT!,'utf8')) as NativeNestedProject
 nestedComposition.document={...project,root:'concat-timeline-0'};nestedComposition.activeCompositionId='concat-timeline-0'
 component=mount(Inspector,{target:document.body});await tick()
 let commits=0
 for(const label of ['Масштаб','Смещение X','Смещение Y','Вращение']){
  document.querySelector<HTMLButtonElement>(`[aria-label="${label}: удалить исходные ключи"]`)!.click()
  const expected=++commits;await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(expected));await tick()
 }
 for(let index=1;index<=2;index++){
  const input=document.querySelector<HTMLInputElement>(`[aria-label="Непрозрачность: ключ ${index} значение"]`)!
  input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}));const expected=++commits
  await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(expected));await tick()
 }
 const track=nestedComposition.document!.compositions['concat-timeline-0']!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track')
 const transform=track.clips[0]!.transform
 expect(transform.concatAnimation!.keys.opacity!.map(key=>key.value)).toEqual([0.5,0.5])
 if(process.env.CONCAT_POSE_EDITED_OUTPUT)writeFileSync(process.env.CONCAT_POSE_EDITED_OUTPUT,JSON.stringify(transform))
})

function installCrop(locked=false){
 const snapshot=JSON.parse(JSON.stringify(nestedComposition.document));snapshot.compositions.root.content.tracks[0].locked=locked
 snapshot.compositions.root.content.tracks[0].clips[0].transform={concatPreparation:{crop:[0,0,0,0],canvas:[16,16],flipH:false,flipV:false}}
 nestedComposition.document=snapshot
}
function crop(){const track=nestedComposition.document!.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');return track.clips[0]!.transform.concatPreparation!}
it('edits crop and flips with source tidying, frame validation and undo/redo',async()=>{
 installCrop();component=mount(Inspector,{target:document.body});await tick()
 const change=async(label:string,value:string,history:number)=>{const input=document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(history));await tick()}
 await change('Обрезка Concat слева','0.6',1);await change('Обрезка Concat справа','0.8',2)
 expect(crop().crop[0]).toBe(0.6);expect(crop().crop[2]).toBeCloseTo(0.3)
 document.querySelector<HTMLInputElement>('[aria-label="Concat отражение по горизонтали"]')!.click();await vi.waitFor(()=>expect(crop().flipH).toBe(true));expect(nestedHistory.past).toHaveLength(3)
 undoNestedComposition();expect(crop().flipH).toBe(false);redoNestedComposition();expect(crop().flipH).toBe(true)
 expect(evaluateNestedFrame).toHaveBeenCalledTimes(3);expect(nestedComposition.renderedOutputs).toEqual({});await tick()
 const reset=[...document.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Сбросить обрезку')!;reset.click();await vi.waitFor(()=>expect(crop().crop).toEqual([0,0,0,0]));expect(crop().flipH).toBe(true)
})
it('preserves crop and history when native frame validation rejects an edit',async()=>{
 installCrop();vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('crop frame failed'));component=mount(Inspector,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Обрезка Concat слева"]')!;input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.error).toContain('crop frame failed'));expect(crop().crop).toEqual([0,0,0,0]);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'stale'})
})
it('disables crop controls for locked tracks',async()=>{
 installCrop(true);component=mount(Inspector,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Обрезка Concat слева"]')!;expect(input.closest('fieldset')!.disabled).toBe(true)
 expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
})

it.skipIf(!process.env.CONCAT_CROP_IMPORTED_INPUT)('captures crop controls from an actual imported descriptor for encoded delivery',async()=>{
 const project=JSON.parse(readFileSync(process.env.CONCAT_CROP_IMPORTED_INPUT!,'utf8')) as NativeNestedProject
 nestedComposition.document={...project,root:'concat-timeline-0'};nestedComposition.activeCompositionId='concat-timeline-0'
 component=mount(Inspector,{target:document.body});await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Обрезка Concat слева"]')!;input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}));await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(1));await tick()
 document.querySelector<HTMLInputElement>('[aria-label="Concat отражение по горизонтали"]')!.click();await vi.waitFor(()=>expect(nestedHistory.past).toHaveLength(2))
 const track=nestedComposition.document!.compositions['concat-timeline-0']!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track')
 const preparation=track.clips[0]!.transform.concatPreparation!
 expect(preparation).toEqual({crop:[0.5,0,0,0],canvas:[16,16],flipH:true,flipV:false})
 if(process.env.CONCAT_CROP_EDITED_OUTPUT)writeFileSync(process.env.CONCAT_CROP_EDITED_OUTPUT,JSON.stringify(preparation))
})
