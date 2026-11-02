import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {evaluateNestedFrame,type NativeNestedProject} from '$lib/api'
import {agentSession} from '$lib/agent/session.svelte'
import {insertNativeConcatTitle} from '$lib/composition/nativeTitleInsert'
import {nestedComposition,nestedHistory,closeNestedComposition,editNestedVideoEffects,reactivateNestedSourceEffect,editNestedConcatPreparation,editNestedConcatClipKeys,undoNestedComposition} from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({evaluateNestedFrame:vi.fn(),upgradeConcatEffectLinks:vi.fn()}))
function project():NativeNestedProject{
 const base:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:320,height:180,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}}
 const output=insertNativeConcatTitle(base,'root',{clipId:'title',trackId:'',newTrackId:'titles',text:'Title',startUs:0,durationUs:2000000}),node=output.compositions.root,track=node.content.tracks[0]
 if(track.kind!=='text')throw new Error('text required')
 node.content={...node.content,tracks:[{...track,clips:track.clips.map(clip=>({...clip,effects:[{kind:'concat_exposure',parameters:{stops:1}}],transform:{...clip.transform,concatPreparation:{crop:[0,0,0,0],canvas:[320,180]},concatAnimation:{durationTicks:2000000,keys:{},rest:{scaleX:1,scaleY:1,x:0,y:0,rotationDegrees:0,opacity:1},stretch:[1,1],canvas:[320,180]}}}))}]}
 return output
}
beforeEach(()=>{closeNestedComposition();agentSession.lock=null;vi.clearAllMocks();vi.mocked(evaluateNestedFrame).mockResolvedValue({} as never);nestedComposition.document=project();nestedComposition.renderedOutputs={root:'old'}})
afterEach(()=>{agentSession.lock=null;closeNestedComposition()})
function title(){const track=nestedComposition.document!.compositions.root.content.tracks[0];if(track.kind!=='text')throw new Error('text required');return track.clips[0]}
it('edits retained title crop, keys and effects through shared validation/history',async()=>{
 const before=JSON.parse(JSON.stringify(nestedComposition.document));await editNestedConcatPreparation('root','titles','title',{kind:'flip',axis:'horizontal',value:true});expect(title().transform.concatPreparation?.flipH).toBe(true);expect(nestedComposition.renderedOutputs).toEqual({});undoNestedComposition();expect(nestedComposition.document).toEqual(before)
 await editNestedConcatClipKeys('root','titles','title','rotation',[{at:0,value:0},{at:1,value:45}]);expect(title().transform.concatAnimation?.keys.rotation).toEqual([{at:0,value:0},{at:1,value:45}]);undoNestedComposition();expect(nestedComposition.document).toEqual(before)
 await reactivateNestedSourceEffect('root','titles','title',0);expect(title().effects?.[0].kind).toBe('native_bypass');await reactivateNestedSourceEffect('root','titles','title',0);expect(title().effects?.[0].kind).toBe('concat_exposure')
 await editNestedVideoEffects('root','titles','title',()=>[]);expect(title().effects).toEqual([]);expect(nestedHistory.error).toBe('')
})
it('rejects ordinary text and late locks without committing candidate edits',async()=>{
 const input=project(),track=input.compositions.root.content.tracks[0];if(track.kind!=='text')throw new Error('text required');delete (track.clips[0] as {concatTitle?:unknown}).concatTitle;nestedComposition.document=input
 await editNestedConcatPreparation('root','titles','title',{kind:'reset-crop'});expect(nestedHistory.error).toContain('Concat');expect(evaluateNestedFrame).not.toHaveBeenCalled()
 nestedComposition.document=project();let finish!:()=>void;vi.mocked(evaluateNestedFrame).mockReturnValue(new Promise(done=>{finish=()=>done({} as never)}));const pending=editNestedConcatPreparation('root','titles','title',{kind:'flip',axis:'vertical',value:true});await vi.waitFor(()=>expect(evaluateNestedFrame).toHaveBeenCalled());agentSession.lock={owner:'other',reason:'Held',expiresAt:Date.now()+60000};finish();await pending;expect(nestedHistory.error).toContain('Held');expect(title().transform.concatPreparation?.flipV).toBeUndefined();expect(nestedHistory.past).toHaveLength(0)
})

it('initializes preparation for a newly authored title only on edit and Undo restores absence',async()=>{
 const input=project(),track=input.compositions.root.content.tracks[0];if(track.kind!=='text')throw new Error('text required')
 delete (track.clips[0].transform as {concatPreparation?:unknown}).concatPreparation;nestedComposition.document=input
 const before=JSON.parse(JSON.stringify(input));await editNestedConcatPreparation('root','titles','title',{kind:'edge',index:0,value:0.15})
 expect(title().transform.concatPreparation).toEqual({crop:[0.15,0,0,0],canvas:[320,180]});expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).toEqual(before);expect(title().transform.concatPreparation).toBeUndefined()
})

it('starts source animation on a title without imported keys, preserves its pose and restores absence',async()=>{
 const input=project(),track=input.compositions.root.content.tracks[0];if(track.kind!=='text')throw new Error('text required')
 delete (track.clips[0].transform as {concatAnimation?:unknown}).concatAnimation
 nestedComposition.document=input;const before=JSON.parse(JSON.stringify(input))
 await editNestedConcatClipKeys('root','titles','title','rotation',[{at:0,value:0},{at:1,value:90}]);expect(title().transform.concatAnimation).toMatchObject({durationTicks:2000000,canvas:[320,180],stretch:[1,1],keys:{rotation:[{at:0,value:0},{at:1,value:90}]}});undoNestedComposition();expect(nestedComposition.document).toEqual(before)
})
