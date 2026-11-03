import { executeAgentRequest } from '$lib/agent/runtime.svelte'
import { agentSession } from '$lib/agent/session.svelte'
import { beforeEach, expect, it, vi } from 'vitest'
import * as api from '$lib/api'
import { addClip, addTrack, createComposition, registerSource } from '$lib/composition/commands'
import { buildCompositionWireDocument } from '$lib/composition/payload'
import { closeNestedComposition, freezeNestedVideoClip, nestedComposition, nestedHistory, undoNestedComposition, redoNestedComposition } from './nestedComposition.svelte'
import { authState } from './auth.svelte'
import NestedClipTransfer from '$lib/components/composition/NestedClipTransfer.svelte'
// @ts-expect-error Tests use the Svelte client entry.
import { mount, tick, unmount } from '../../../node_modules/svelte/src/index-client.js'
vi.mock('$lib/api',()=>({getNestedCompositionProject:vi.fn(),freezeCompositionProject:vi.fn(),nestedDurations:vi.fn(),getLibrary:vi.fn(async()=>[]),evaluateNestedFrame:vi.fn(async()=>({}))}))
const clone=<T>(value:T):T=>JSON.parse(JSON.stringify(value)) as T
function setup(){
 let c=registerSource(createComposition({width:16,height:16,fps:25,backgroundColor:'#000000'}),{id:'source',kind:'video',durationTicks:4000000,width:16,height:16,hasAudio:false})
 c=addTrack(c,{id:'v',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[]})
 c=addClip(c,'v',{id:'clip',kind:'video',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:4000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1})
 const document:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(c),instances:[]}}}
 const saved:api.NestedCompositionProjectDto={id:'project',revision:3,name:'Saved',schemaVersion:2,mode:'composition',document:clone(document),sourceIds:['source'],createdAt:0,updatedAt:0}
 const content=document.compositions.root!.content
 const video=content.tracks[0];if(video?.kind!=='video')throw new Error('fixture video missing')
 const picture=video.clips[0]!
 const changed:api.NativeNestedProject={...clone(document),compositions:{root:{...document.compositions.root!,content:{...content,sources:{...content.sources,still:{id:'still',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}},tracks:[{id:'image',name:'Still',kind:'image',hidden:false,locked:false,clips:[{id:'hold',sourceId:'still',timelineStartTick:1000000,durationTicks:1000000,transform:clone(picture.transform),opacity:clone(picture.opacity),blendMode:picture.blendMode,effects:[],enabled:true}]},...content.tracks]}}}}
 const result:api.FreezeCompositionProjectResult={changed:true,project:{...saved,revision:4,document:changed,sourceIds:['source','still']},media:{id:'still',kind:'source',filename:'still.jpg',url:'/files/sources/still.jpg',mediaType:'image',width:16,height:16,createdAt:0}}
 nestedComposition.document=clone(document);nestedComposition.projectId=saved.id;nestedComposition.revision=3;nestedComposition.name='Saved';nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1000000;nestedComposition.renderedOutputs={root:'old-movie'}
 vi.mocked(api.getNestedCompositionProject).mockResolvedValue(saved);vi.mocked(api.freezeCompositionProject).mockResolvedValue(result)
 return {document,saved,result,changed}
}
beforeEach(()=>{closeNestedComposition();vi.clearAllMocks();authState.token=null})
it('commits the saved response once, preserving the server revision across local undo and redo',async()=>{
 const {document,changed}=setup()
 expect(await freezeNestedVideoClip('token','root','clip')).toBe('hold')
 expect(api.freezeCompositionProject).toHaveBeenCalledWith('project',{revision:3,compositionId:'root',clipId:'clip',time:1},'token')
 expect(nestedComposition.document).toEqual(changed);expect(nestedComposition.revision).toBe(4);expect(nestedComposition.renderedOutputs).toEqual({});expect(nestedHistory.past).toHaveLength(1)
 undoNestedComposition();expect(nestedComposition.document).toEqual(document);expect(nestedComposition.revision).toBe(4);expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'})
 redoNestedComposition();expect(nestedComposition.document).toEqual(changed);expect(nestedHistory.past).toHaveLength(1)
})
it('does not send unsaved documents or record a no-op',async()=>{
 const {document}=setup();nestedComposition.document={...document,root:'unsaved'}
 expect(await freezeNestedVideoClip('token','root','clip')).toBeNull();expect(api.freezeCompositionProject).not.toHaveBeenCalled();expect(nestedHistory.error).toContain('Сначала сохраните')
 setup();vi.mocked(api.freezeCompositionProject).mockResolvedValue({changed:false})
 expect(await freezeNestedVideoClip('token','root','clip')).toBeNull();expect(nestedHistory.past).toEqual([]);expect(nestedComposition.revision).toBe(3)
})
it('retains local edits and warns when the server commits while its reply is pending',async()=>{
 const {document,result}=setup();let resolve!:(value:api.FreezeCompositionProjectResult)=>void
 vi.mocked(api.freezeCompositionProject).mockReturnValue(new Promise(done=>{resolve=done}))
 const pending=freezeNestedVideoClip('token','root','clip');await vi.waitFor(()=>expect(api.freezeCompositionProject).toHaveBeenCalled())
 const local={...document,root:'local-edit'};nestedComposition.document=local;resolve(result)
 expect(await pending).toBeNull();expect(nestedComposition.document).toEqual(local);expect(nestedComposition.revision).toBe(3);expect(nestedHistory.past).toEqual([]);expect(nestedHistory.error).toContain('сохранён на сервере');expect(nestedHistory.busy).toBe(false)
})
it('discards a pending result after closing the project and rejects mismatched revisions',async()=>{
 const {result}=setup();let resolve!:(value:api.FreezeCompositionProjectResult)=>void
 vi.mocked(api.freezeCompositionProject).mockReturnValue(new Promise(done=>{resolve=done}))
 const pending=freezeNestedVideoClip('token','root','clip');await vi.waitFor(()=>expect(api.freezeCompositionProject).toHaveBeenCalled())
 closeNestedComposition();resolve(result);await pending;expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toEqual([])
 const next=setup();if(!next.result.changed)throw new Error('fixture changed');next.result.project.revision=9;vi.mocked(api.freezeCompositionProject).mockResolvedValue(next.result)
 expect(await freezeNestedVideoClip('token','root','clip')).toBeNull();expect(nestedHistory.error).toContain('другой проект или ревизию');expect(nestedComposition.revision).toBe(3)
})
it('enables the button for one saved video and selects the new still after a click',async()=>{
 setup();const target=window.document.createElement('div');window.document.body.append(target)
 const component=mount(NestedClipTransfer,{target})
 try{
  await tick();const button=[...target.querySelectorAll<HTMLButtonElement>('button')].find(button=>button.textContent==='Стоп-кадр по курсору')!
  expect(button.disabled).toBe(true);authState.token='token'
  const checkbox=target.querySelector<HTMLInputElement>('[data-clip-id="clip"]')!;checkbox.checked=true;checkbox.dispatchEvent(new Event('change',{bubbles:true}));await tick()
  expect(button.disabled).toBe(false);button.click();await vi.waitFor(()=>expect(nestedComposition.revision).toBe(4));await tick()
  expect(target.querySelector<HTMLInputElement>('[data-clip-id="hold"]')!.checked).toBe(true);expect(nestedHistory.past).toHaveLength(1)
 }finally{unmount(component);target.remove();authState.token=null}
})


it.each(['history', 'cursor', 'scope'] as const)('retains local state if %s changes after server publication starts', async kind => {
 const { result }=setup();const before=nestedComposition.document
 let finish!:(value:api.FreezeCompositionProjectResult)=>void
 vi.mocked(api.freezeCompositionProject).mockReturnValue(new Promise(resolve=>{finish=resolve}))
 const pending=freezeNestedVideoClip('token','root','clip')
 await vi.waitFor(()=>expect(api.freezeCompositionProject).toHaveBeenCalled())
 if(kind==='history')agentSession.lock={owner:'other-agent',reason:'busy',expiresAt:Date.now()+60000}
 else if(kind==='cursor')nestedComposition.timeUs=2000000
 else nestedComposition.activeCompositionId='other'
 finish(result)
 expect(await pending).toBeNull();expect(nestedComposition.document).toBe(before)
 expect(nestedComposition.revision).toBe(3);expect(nestedHistory.past).toHaveLength(0)
 expect(nestedHistory.error).toContain('сохранён на сервере')
})
async function agentRead(){return await executeAgentRequest('freeze-agent','get_project',{}) as {projectHash:string}}
it('publishes through the agent with explicit time, an owned lock, new hash and one Undo entry', async()=>{
 const {changed}=setup();nestedComposition.timeUs=2000000;const {projectHash}=await agentRead()
 agentSession.lock={owner:'freeze-agent',reason:'freeze',expiresAt:Date.now()+60000}
 const result=await executeAgentRequest('freeze-agent','freeze_frame',{expectedProjectHash:projectHash,clipId:'clip',time_us:1000000},{token:'agent-token'}) as {changed:boolean;stillId:string;projectHash:string;revision:number}
 expect(result).toMatchObject({changed:true,stillId:'hold',revision:4});expect(result.projectHash).not.toBe(projectHash)
 expect(api.freezeCompositionProject).toHaveBeenCalledWith('project',{revision:3,compositionId:'root',clipId:'clip',time:1},'agent-token')
 expect(nestedComposition.document).toEqual(changed);expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.timeUs).toBe(2000000)
 agentSession.lock=null;undoNestedComposition();expect(nestedComposition.revision).toBe(4)
})
it('refuses stale hash, absent authentication, locked targets and unsupported arguments before server writes', async()=>{
 setup();const {projectHash}=await agentRead()
 const args={expectedProjectHash:projectHash,clipId:'clip',time_us:1000000}
 await expect(executeAgentRequest('freeze-agent','freeze_frame',{...args,expectedProjectHash:'0'.repeat(64)},{token:'token'})).rejects.toThrow('Project changed')
 await expect(executeAgentRequest('freeze-agent','freeze_frame',args)).rejects.toThrow('authenticated saved')
 await expect(executeAgentRequest('freeze-agent','freeze_frame',{...args,path:'/forged.jpg'},{token:'token'})).rejects.toThrow('Invalid freeze_frame')
 Object.assign(nestedComposition.document!.compositions.root!.content.tracks[0]!,{locked:true})
 const locked=await agentRead()
 await expect(executeAgentRequest('freeze-agent','freeze_frame',{...args,expectedProjectHash:locked.projectHash},{token:'token'})).rejects.toThrow('unlocked native video')
 expect(api.freezeCompositionProject).not.toHaveBeenCalled()
})
it('does not publish an agent request that expires during saved-document preflight',async()=>{
 const {saved}=setup();const {projectHash}=await agentRead();let active=true
 let finish!:(value:api.NestedCompositionProjectDto)=>void
 vi.mocked(api.getNestedCompositionProject).mockReturnValue(new Promise(resolve=>{finish=resolve}))
 const pending=executeAgentRequest('freeze-agent','freeze_frame',{expectedProjectHash:projectHash,clipId:'clip',time_us:1000000},{token:'token',isActive:()=>active})
 await vi.waitFor(()=>expect(api.getNestedCompositionProject).toHaveBeenCalled());active=false;finish(saved)
 await expect(pending).rejects.toThrow('expired or disconnected');expect(api.freezeCompositionProject).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0)
})
it('reports committed server state without applying a reply to an expired agent request',async()=>{
 const {result}=setup();const {projectHash}=await agentRead();let active=true
 const before=nestedComposition.document;let finish!:(value:api.FreezeCompositionProjectResult)=>void
 vi.mocked(api.freezeCompositionProject).mockReturnValue(new Promise(resolve=>{finish=resolve}))
 const pending=executeAgentRequest('freeze-agent','freeze_frame',{expectedProjectHash:projectHash,clipId:'clip',time_us:1000000},{token:'token',isActive:()=>active})
 await vi.waitFor(()=>expect(api.freezeCompositionProject).toHaveBeenCalled());active=false;finish(result)
 await expect(pending).rejects.toThrow('сохранён на сервере')
 expect(nestedComposition.document).toBe(before);expect(nestedComposition.revision).toBe(3);expect(nestedHistory.past).toHaveLength(0)
})

it('returns an unchanged hash and no Undo entry for a server no-op',async()=>{
 setup();const {projectHash}=await agentRead()
 vi.mocked(api.freezeCompositionProject).mockResolvedValueOnce({changed:false})
 const result=await executeAgentRequest('freeze-agent','freeze_frame',{expectedProjectHash:projectHash,clipId:'clip',time_us:0},{token:'token'})
 expect(result).toMatchObject({changed:false,stillId:null,projectHash,revision:3})
 expect(nestedHistory.past).toHaveLength(0);expect(nestedHistory.error).toBe('')
})

it('reports server publication to the agent even if the editor closes while the response is pending',async()=>{
 const {result}=setup();const {projectHash}=await agentRead()
 let finish!:(value:api.FreezeCompositionProjectResult)=>void
 vi.mocked(api.freezeCompositionProject).mockReturnValue(new Promise(resolve=>{finish=resolve}))
 const pending=executeAgentRequest('freeze-agent','freeze_frame',{expectedProjectHash:projectHash,clipId:'clip',time_us:1000000},{token:'token'})
 await vi.waitFor(()=>expect(api.freezeCompositionProject).toHaveBeenCalled())
 closeNestedComposition();finish(result)
 await expect(pending).rejects.toThrow('сохранён на сервере')
 expect(nestedComposition.document).toBeNull();expect(nestedHistory.past).toHaveLength(0)
})

it('keeps logical media bindings through saved freeze adoption and one Undo/Redo',async()=>{
 const {document,saved,result,changed}=setup();if(!result.changed)throw new Error('fixture changed')
 const source=document.compositions.root!.content.sources.source!
 const records={source:{source,librarySourceId:'physical-video',name:'Slot',placeholder:true},alias:{source:{...source,id:'alias'},librarySourceId:'physical-video',name:'Other slot',placeholder:true}}
 document.projectMedia=clone(records);saved.document.projectMedia=clone(records);changed.projectMedia=clone(records)
 saved.sourceIds=['physical-video'];result.project.sourceIds=['physical-video','still'];nestedComposition.document=clone(document)
 expect(await freezeNestedVideoClip('token','root','clip')).toBe('hold')
 expect(nestedComposition.document?.projectMedia).toEqual(records);expect(nestedComposition.revision).toBe(4);expect(nestedHistory.past).toHaveLength(1)
 undoNestedComposition();expect(nestedComposition.document).toEqual(document);expect(nestedComposition.document?.projectMedia).toEqual(records)
 redoNestedComposition();expect(nestedComposition.document).toEqual(changed);expect(nestedComposition.document?.compositions.root!.content.tracks.filter(track=>track.kind==='video').flatMap(track=>track.clips).every(clip=>clip.sourceId==='source')).toBe(true)
})
