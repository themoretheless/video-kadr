import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {upgradeConcatEffectLinks,evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {agentSession} from '$lib/agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,insertNestedConcatTreatment,addNestedConcatTreatmentEffect,undoNestedComposition,redoNestedComposition} from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn(),upgradeConcatEffectLinks:vi.fn()}))
const project:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}}
const request={clipId:'look',trackId:'',newTrackId:'adjustment',effectId:'concat.exposure',startUs:0,durationUs:3000000}
const resolved={schemaVersion:1 as const,links:[{upgraded:false,link:{id:'concat.exposure'}}],nativeEffects:[{kind:'concat_exposure' as const,parameters:{stops:0}}]}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();vi.mocked(upgradeConcatEffectLinks).mockResolvedValue(resolved);vi.mocked(evaluateNestedFrame).mockResolvedValue({} as never);nestedComposition.document=project;nestedComposition.renderedOutputs={root:'cached'}})
afterEach(()=>{agentSession.lock=null;closeNestedComposition()})
it('resolves original package defaults before creating one history entry',async()=>{
 await insertNestedConcatTreatment('root',request);expect(upgradeConcatEffectLinks).toHaveBeenCalledWith([{id:'concat.exposure',enabled:true}],undefined,true);expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.document!.compositions.root.content.tracks[0].clips[0]).toMatchObject({concatTreatment:{strength:1,durationTicks:3000000},effects:resolved.nativeEffects});expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(nestedComposition.document).toEqual(project);expect(nestedComposition.renderedOutputs).toEqual({root:'cached'});redoNestedComposition();expect(nestedComposition.document!.compositions.root.content.tracks).toHaveLength(1)
})
it('refuses unavailable packages and late locks without leaving an empty lane',async()=>{
 vi.mocked(upgradeConcatEffectLinks).mockResolvedValue({...resolved,nativeEffects:[null]});await insertNestedConcatTreatment('root',request);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(project)
 let finish!:(value:typeof resolved)=>void;vi.mocked(upgradeConcatEffectLinks).mockReturnValue(new Promise(done=>{finish=done}));const pending=insertNestedConcatTreatment('root',request);await vi.waitFor(()=>expect(upgradeConcatEffectLinks).toHaveBeenCalledTimes(2));agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};finish(resolved);await pending;expect(nestedHistory.error).toContain('Held');expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedComposition.document).toEqual(project)
})

it('refuses an unavailable append and a lock acquired while resolving the package without altering the prefix',async()=>{
 await insertNestedConcatTreatment('root',request);const before=JSON.parse(JSON.stringify(nestedComposition.document));vi.clearAllMocks();vi.mocked(upgradeConcatEffectLinks).mockResolvedValue({...resolved,nativeEffects:[null]});await addNestedConcatTreatmentEffect('root','adjustment','look','unknown');expect(nestedComposition.document).toEqual(before);expect(nestedHistory.past).toHaveLength(1);expect(evaluateNestedFrame).not.toHaveBeenCalled()
 let finish!:(value:typeof resolved)=>void;vi.mocked(upgradeConcatEffectLinks).mockReturnValue(new Promise(done=>{finish=done}));const pending=addNestedConcatTreatmentEffect('root','adjustment','look','concat.exposure');await vi.waitFor(()=>expect(upgradeConcatEffectLinks).toHaveBeenCalledTimes(2));agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};finish(resolved);await pending;expect(nestedHistory.error).toContain('Held');expect(nestedComposition.document).toEqual(before);expect(nestedHistory.past).toHaveLength(1);expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
