import {expect,it} from 'vitest'
import {insertNativeConcatTreatment} from './nativeTreatmentInsert'
import type {NativeNestedProject} from '$lib/api'
const project=():NativeNestedProject=>({schemaVersion:1,root:'root',compositions:{root:{instances:[],durationUs:1000000,content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}})
const request={clipId:'look',trackId:'',newTrackId:'adjustment',effects:[{kind:'concat_exposure' as const,parameters:{stops:1}}],startUs:500000,durationUs:3000000}
it('creates a source-free retained treatment with a detached effect chain and extends the target duration',()=>{
 const input=project(),before=JSON.stringify(input),output=insertNativeConcatTreatment(input,'root',request);expect(JSON.stringify(input)).toBe(before);expect(output.compositions.root.durationUs).toBe(3500000);expect(output.compositions.root.content.sources).toEqual({});expect(output.compositions.root.content.tracks[0]).toMatchObject({kind:'graphic',clips:[{id:'look',graphic:{layers:[]},concatTreatment:{strength:1,fadeIn:0,fadeOut:0,durationTicks:3000000},effects:request.effects}]});request.effects[0].parameters.stops=2;expect(output.compositions.root.content.tracks[0].clips[0]).toMatchObject({effects:[{parameters:{stops:1}}]});request.effects[0].parameters.stops=1
})
it('rejects locked tracks, identity conflicts and empty effect chains without mutation',()=>{
 const input=insertNativeConcatTreatment(project(),'root',request),before=JSON.stringify(input)
 for(const patch of [{clipId:'look'},{clipId:'other',durationUs:0},{clipId:'other',startUs:-1},{clipId:'other',effects:[]}])expect(()=>insertNativeConcatTreatment(input,'root',{...request,...patch})).toThrow()
 const track=input.compositions.root.content.tracks[0],locked={...input,compositions:{root:{...input.compositions.root,content:{...input.compositions.root.content,tracks:[{...track,locked:true}]}}}};expect(()=>insertNativeConcatTreatment(locked,'root',{...request,clipId:'other',trackId:'adjustment',startUs:4000000})).toThrow('заблокирована');expect(JSON.stringify(input)).toBe(before)
 const next=insertNativeConcatTreatment(input,'root',{...request,clipId:'other',trackId:'adjustment',startUs:3500000});expect(next.compositions.root.content.tracks).toHaveLength(1)
})
