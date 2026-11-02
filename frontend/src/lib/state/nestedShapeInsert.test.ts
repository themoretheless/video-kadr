import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {insertNativeConcatShape} from '$lib/composition/nativeShapeInsert'
import {agentSession} from '$lib/agent/session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,insertNestedConcatShape,editNestedConcatPreparation,editNestedConcatClipKeys,editNestedVideoEffects,reactivateNestedSourceEffect,undoNestedComposition,redoNestedComposition} from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn(),upgradeConcatEffectLinks:vi.fn()}))
const project=():NativeNestedProject=>({schemaVersion:1,root:'root',compositions:{root:{instances:[],durationUs:1000000,content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}})
const request={clipId:'shape',trackId:'',newTrackId:'shapes',kind:'circle' as const,startUs:0,durationUs:3000000}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({} as never);nestedComposition.document=project();nestedComposition.renderedOutputs={root:'cached'}})
afterEach(()=>{agentSession.lock=null;closeNestedComposition()})
it('commits lane/clip/duration once and restores output bindings with Undo/Redo',async()=>{
 const before=JSON.parse(JSON.stringify(nestedComposition.document));await insertNestedConcatShape('root',request);expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.document!.compositions.root.durationUs).toBe(3000000);expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(nestedComposition.document).toEqual(before);expect(nestedComposition.renderedOutputs).toEqual({root:'cached'});redoNestedComposition();expect(nestedComposition.document!.compositions.root.content.tracks).toHaveLength(1)
})
it('refuses initial and late history locks without adding a lane',async()=>{
 agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};await insertNestedConcatShape('root',request);expect(evaluateNestedFrame).not.toHaveBeenCalled();expect(nestedHistory.error).toContain('Held')
 agentSession.lock=null;let finish!:()=>void;vi.mocked(evaluateNestedFrame).mockReturnValue(new Promise(done=>{finish=()=>done({} as never)}));const pending=insertNestedConcatShape('root',request);await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());agentSession.lock={owner:'other',reason:'Late',expiresAt:Date.now()+60000};finish();await pending;expect(nestedHistory.error).toContain('Late');expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document!.compositions.root.content.tracks).toHaveLength(0)
})

function shape(){const track=nestedComposition.document!.compositions.root.content.tracks[0];if(track.kind!=='graphic')throw new Error('graphic required');return track.clips[0]}
it('initializes shape crop and source-motion keys and restores exact absence through Undo',async()=>{
 nestedComposition.document=insertNativeConcatShape(project(),'root',request);const before=JSON.parse(JSON.stringify(nestedComposition.document))
 await editNestedConcatPreparation('root','shapes','shape',{kind:'flip',axis:'horizontal',value:true});expect(shape().transform.concatPreparation).toEqual({crop:[0,0,0,0],canvas:[160,120],flipH:true});undoNestedComposition();expect(nestedComposition.document).toEqual(before)
 await editNestedConcatClipKeys('root','shapes','shape','rotation',[{at:0,value:0},{at:1,value:45}]);expect(shape().transform.concatAnimation).toMatchObject({durationTicks:3000000,canvas:[160,120],keys:{rotation:[{at:0,value:0},{at:1,value:45}]}});undoNestedComposition();expect(nestedComposition.document).toEqual(before)
})
it('edits shape effects and bypass state while rejecting ordinary graphic clips',async()=>{
 nestedComposition.document=insertNativeConcatShape(project(),'root',request);await editNestedVideoEffects('root','shapes','shape',()=>[{kind:'concat_exposure',parameters:{stops:1}}]);expect(shape().effects).toHaveLength(1)
 await reactivateNestedSourceEffect('root','shapes','shape',0);expect(shape().effects[0].kind).toBe('native_bypass');await reactivateNestedSourceEffect('root','shapes','shape',0);expect(shape().effects[0].kind).toBe('concat_exposure')
 delete (shape() as {concatShape?:unknown}).concatShape;vi.clearAllMocks();await editNestedConcatPreparation('root','shapes','shape',{kind:'reset-crop'});expect(nestedHistory.error).toContain('фигура Concat');expect(evaluateNestedFrame).not.toHaveBeenCalled()
})
