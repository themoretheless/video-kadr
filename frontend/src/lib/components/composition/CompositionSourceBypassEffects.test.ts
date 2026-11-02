import {readFileSync,writeFileSync} from 'node:fs'
// @ts-expect-error Client entry needed for DOM tests.
import {mount,tick,unmount} from '../../../../node_modules/svelte/src/index-client.js'
import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {upgradeConcatEffectLinks,evaluateNestedFrame} from '$lib/api'
import {addMediaInfoToComposition,commitDocument,compositionState,resetCompositionForTests,undoComposition,redoComposition,reactivateCompositionSourceEffect,setCompositionClipLocked} from '$lib/state/composition.svelte'
import Inspector from './CompositionSourceBypassEffects.svelte'
import PoseEditor from './CompositionConcatClipAnimation.svelte'
import {editCompositionConcatClipKeys} from '$lib/state/composition.svelte'
vi.mock('$lib/api',async original=>({...await original<typeof import('$lib/api')>(),upgradeConcatEffectLinks:vi.fn(),evaluateNestedFrame:vi.fn()}))
let component:ReturnType<typeof mount>|undefined,id=''
function response():Awaited<ReturnType<typeof upgradeConcatEffectLinks>>{
 if(process.env.VISUAL_BYPASS_UPGRADE_INPUT)return JSON.parse(readFileSync(process.env.VISUAL_BYPASS_UPGRADE_INPUT,'utf8'))
 return {schemaVersion:1,links:[{upgraded:false,link:{id:'concat.exposure',enabled:true,params:{stops:4},keys:{stops:[{at:0,value:0},{at:1,value:4,ease:'inOut'}]}}}],nativeEffects:[{kind:'concat_exposure',parameters:{stops:4},concatKeys:[{at:0,value:0},{at:1,value:4,ease:[0.42,0,0.58,1]}]}]}
}
function clip(){const found=compositionState.document.tracks.flatMap<import('$lib/composition/types').CompositionClip>(track=>track.clips).find(clip=>clip.id===id)!;if(found.kind!=='video')throw new Error('video');return found}
beforeEach(()=>{
 resetCompositionForTests();vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({entries:[]})
 id=addMediaInfoToComposition({id:'source',url:'/files/sources/source.mov',filename:'source.mov',mediaType:'video',duration:1,width:16,height:16},{pairAudio:false})
 const link={...response().links[0]!.link,enabled:false as const}
 commitDocument({...compositionState.document,tracks:compositionState.document.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===id?{...clip,effects:[{kind:'concat_bypass' as const,link}]}:clip)}:track)})
})
afterEach(async()=>{if(component)await unmount(component);component=undefined;resetCompositionForTests();document.body.innerHTML=''})
async function show(){component=mount(Inspector,{target:document.body,props:{clipId:id,effects:clip().effects}});await tick();document.querySelector<HTMLButtonElement>('[aria-label="Включить исходный эффект 1"]')!.click()}
it('reactivates ordinary source effects through validated history and exports their keys',async()=>{
 const result=response();vi.mocked(upgradeConcatEffectLinks).mockResolvedValue(result);const before=compositionState.history.past.length;await show();await vi.waitFor(()=>expect(compositionState.history.past).toHaveLength(before+1))
 expect(clip().effects![0]).toEqual(result.nativeEffects![0]);expect(evaluateNestedFrame).toHaveBeenCalledOnce();expect(upgradeConcatEffectLinks).toHaveBeenCalledWith([{...result.links[0]!.link,enabled:true}],undefined,true)
 if(process.env.ORDINARY_VISUAL_BYPASS_REACTIVATED_OUTPUT)writeFileSync(process.env.ORDINARY_VISUAL_BYPASS_REACTIVATED_OUTPUT,JSON.stringify(clip().effects![0]))
 if(process.env.ORDINARY_VISUAL_BYPASS_PROJECT_OUTPUT)writeFileSync(process.env.ORDINARY_VISUAL_BYPASS_PROJECT_OUTPUT,JSON.stringify(vi.mocked(evaluateNestedFrame).mock.calls[0]![0]))
 undoComposition();expect(clip().effects![0]!.kind).toBe('concat_bypass');redoComposition();expect(clip().effects![0]).toEqual(result.nativeEffects![0])
})
it('keeps unsupported effects unchanged and reports why',async()=>{
 vi.mocked(upgradeConcatEffectLinks).mockResolvedValue({schemaVersion:1,links:response().links,nativeEffects:[null]});const before=JSON.stringify(compositionState.document),past=compositionState.history.past.length;await show();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('пока нельзя включить'));expect(JSON.stringify(compositionState.document)).toBe(before);expect(compositionState.history.past).toHaveLength(past);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('discards conversion when the selection changes without editing the document',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof upgradeConcatEffectLinks>>)=>void;vi.mocked(upgradeConcatEffectLinks).mockReturnValue(new Promise(done=>resolve=done));const past=compositionState.history.past.length;await show();await vi.waitFor(()=>expect(upgradeConcatEffectLinks).toHaveBeenCalled());compositionState.ui.selectedClipId=null;resolve(response());await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('Выбранный клип изменился'));expect(clip().effects![0]!.kind).toBe('concat_bypass');expect(compositionState.history.past).toHaveLength(past);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
it('refuses locked clips before conversion',async()=>{
 setCompositionClipLocked(id,true);await expect(reactivateCompositionSourceEffect(id,0)).rejects.toThrow('заблокированы');expect(upgradeConcatEffectLinks).not.toHaveBeenCalled()
})
it('keeps the saved source descriptor when native validation fails',async()=>{
 vi.mocked(upgradeConcatEffectLinks).mockResolvedValue(response());vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('invalid native candidate'));const past=compositionState.history.past.length;await show();await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('invalid native candidate'));expect(clip().effects![0]!.kind).toBe('concat_bypass');expect(compositionState.history.past).toHaveLength(past)
})
it('does not restore an old project when conversion finishes after reset',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof upgradeConcatEffectLinks>>)=>void;vi.mocked(upgradeConcatEffectLinks).mockReturnValue(new Promise(done=>resolve=done));await show();await vi.waitFor(()=>expect(upgradeConcatEffectLinks).toHaveBeenCalled());resetCompositionForTests();const replacement=compositionState.document;resolve(response());await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('Проект изменился'));expect(compositionState.document).toBe(replacement);expect(compositionState.history.past).toHaveLength(0);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})

it('repeatedly bypasses native keyed effects without losing descriptors and supports undo',async()=>{
 const native=response().nativeEffects![0]!;
 commitDocument({...compositionState.document,tracks:compositionState.document.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===id?{...clip,effects:[native]}:clip)}:track)})
 const past=compositionState.history.past.length
 for(let iteration=0;iteration<3;iteration++){
  await reactivateCompositionSourceEffect(id,0)
  expect(clip().effects![0]).toEqual({kind:'native_bypass',effect:native})
  await reactivateCompositionSourceEffect(id,0)
  expect(clip().effects![0]).toEqual(native)
 }
 expect(upgradeConcatEffectLinks).not.toHaveBeenCalled()
 expect(compositionState.history.past).toHaveLength(past+6)
 undoComposition();expect(clip().effects![0]).toEqual({kind:'native_bypass',effect:native})
 redoComposition();expect(clip().effects![0]).toEqual(native)
})

it('does not commit a native toggle when the project changes during frame validation',async()=>{
 const native=response().nativeEffects![0]!
 commitDocument({...compositionState.document,tracks:compositionState.document.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===id?{...clip,effects:[native]}:clip)}:track)})
 let resolve!:(value:Awaited<ReturnType<typeof evaluateNestedFrame>>)=>void
 vi.mocked(evaluateNestedFrame).mockReturnValueOnce(new Promise(done=>resolve=done))
 const pending=reactivateCompositionSourceEffect(id,0)
 await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());resetCompositionForTests();const replacement=compositionState.document;resolve({entries:[]})
 await expect(pending).rejects.toThrow('Проект изменился');expect(compositionState.document).toBe(replacement);expect(compositionState.history.past).toHaveLength(0)
})

function installPose(){
 const animation:import('$lib/composition/types').WireConcatClipAnimation={durationTicks:1000000,keys:{opacity:[{at:0,value:0},{at:1,value:1}]},rest:{scaleX:1,scaleY:1,x:0,y:0,rotationDegrees:0,opacity:0},stretch:[1,1],canvas:[16,16]}
 commitDocument({...compositionState.document,tracks:compositionState.document.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===id?{...clip,concatAnimation:animation,effects:[]}:clip)}:track)})
 return animation
}
async function changePose(value='0.6'){
 const animation=installPose();component=mount(PoseEditor,{target:document.body,props:{clipId:id,animation}});await tick()
 const input=document.querySelector<HTMLInputElement>('[aria-label="Непрозрачность: ключ 2 значение"]')!;input.value=value;input.dispatchEvent(new Event('change',{bubbles:true}))
}
it('edits ordinary source pose keys with validated payload and undo/redo',async()=>{
 await changePose();await vi.waitFor(()=>expect(clip().concatAnimation!.keys.opacity![1]!.value).toBe(0.6))
 const payload=vi.mocked(evaluateNestedFrame).mock.calls[0]![0]
 expect(payload.compositions.root!.content.tracks[0]).toMatchObject({clips:[{transform:{concatAnimation:{keys:{opacity:[{value:0},{value:0.6}]}}}}]})
 undoComposition();expect(clip().concatAnimation!.keys.opacity![1]!.value).toBe(1);redoComposition();expect(clip().concatAnimation!.keys.opacity![1]!.value).toBe(0.6)
})
it('keeps ordinary source keys unchanged when frame validation fails',async()=>{
 vi.mocked(evaluateNestedFrame).mockRejectedValueOnce(new Error('pose render failed'));await changePose();const past=compositionState.history.past.length
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('pose render failed'))
 expect(clip().concatAnimation!.keys.opacity![1]!.value).toBe(1);expect(compositionState.history.past).toHaveLength(past)
})
it('discards ordinary source pose edits when selection changes during validation',async()=>{
 let resolve!:(value:Awaited<ReturnType<typeof evaluateNestedFrame>>)=>void;vi.mocked(evaluateNestedFrame).mockReturnValueOnce(new Promise(done=>resolve=done))
 await changePose();await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());const past=compositionState.history.past.length;compositionState.ui.selectedClipId=null;resolve({entries:[]})
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('Выбранный клип изменился'));expect(clip().concatAnimation!.keys.opacity![1]!.value).toBe(1);expect(compositionState.history.past).toHaveLength(past)
})
it('refuses ordinary source pose edits on locked clips before frame evaluation',async()=>{
 installPose();setCompositionClipLocked(id,true);await expect(editCompositionConcatClipKeys(id,'opacity',[{at:0,value:0.5}])).rejects.toThrow('заблокированы');expect(evaluateNestedFrame).not.toHaveBeenCalled()
})

it.skipIf(!process.env.ORDINARY_CONCAT_POSE_IMPORTED_INPUT)('captures ordinary inspector edits based on real imported source keys',async()=>{
 const imported=JSON.parse(readFileSync(process.env.ORDINARY_CONCAT_POSE_IMPORTED_INPUT!,'utf8')) as import('$lib/api').NativeNestedProject
 const importedTrack=imported.compositions['concat-timeline-0']!.content.tracks[0]!;if(importedTrack.kind!=='video')throw new Error('track')
 const animation=importedTrack.clips[0]!.transform.concatAnimation!
 commitDocument({...compositionState.document,tracks:compositionState.document.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id===id?{...clip,concatAnimation:animation,effects:[]}:clip)}:track)})
 component=mount(PoseEditor,{target:document.body,props:{clipId:id,animation}});await tick()
 let commits=compositionState.history.past.length
 for(const label of ['Масштаб','Смещение X','Смещение Y','Вращение']){
  document.querySelector<HTMLButtonElement>(`[aria-label="${label}: удалить исходные ключи"]`)!.click();const expected=++commits
  await vi.waitFor(()=>expect(compositionState.history.past).toHaveLength(expected));await tick()
 }
 // A real inspector rerenders the animation prop after each history commit.
 // The standalone panel is remounted here with that same committed model.
 if(component)await unmount(component)
 component=mount(PoseEditor,{target:document.body,props:{clipId:id,animation:clip().concatAnimation!}});await tick()
 for(let index=1;index<=2;index++){
  const input=document.querySelector<HTMLInputElement>(`[aria-label="Непрозрачность: ключ ${index} значение"]`)!
  input.value='0.5';input.dispatchEvent(new Event('change',{bubbles:true}));const expected=++commits
  await vi.waitFor(()=>expect(compositionState.history.past).toHaveLength(expected));await tick()
  if(index===1){await unmount(component);component=mount(PoseEditor,{target:document.body,props:{clipId:id,animation:clip().concatAnimation!}});await tick()}
 }
 const candidate=vi.mocked(evaluateNestedFrame).mock.calls.at(-1)![0]
 const track=candidate.compositions.root!.content.tracks[0]!;if(track.kind!=='video')throw new Error('track')
 const transform=track.clips[0]!.transform
 expect(transform.concatAnimation!.keys.opacity!.map(key=>key.value)).toEqual([0.5,0.5])
 if(process.env.ORDINARY_CONCAT_POSE_EDITED_OUTPUT)writeFileSync(process.env.ORDINARY_CONCAT_POSE_EDITED_OUTPUT,JSON.stringify(transform))
})
