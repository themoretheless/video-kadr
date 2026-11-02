import {beforeEach,expect,it,vi} from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import * as api from '$lib/api'
import {attachNestedSavedVideoSmartRegion,startNestedSavedVideoSmartRegion,setNestedSavedVideoCutout,undoNestedComposition,redoNestedComposition,analyseNestedSavedVideoCutout,previewNestedSavedVideoCutout,closeNestedComposition,nestedComposition,nestedHistory} from './nestedComposition.svelte'
vi.mock('$lib/api',()=>({getNestedCompositionProject:vi.fn(),startNestedVideoSmartRegion:vi.fn(),attachNestedVideoSmartRegion:vi.fn(),analyseNestedVideoCutout:vi.fn(),previewNestedVideoCutout:vi.fn(),setNestedVideoCutoutBinding:vi.fn(),evaluateNestedFrame:vi.fn()}))
beforeEach(()=>{closeNestedComposition();vi.resetAllMocks()})
function setup(){const document=structuredClone(fixture.createdTimeline) as unknown as api.NativeNestedProject;Object.assign(nestedComposition,{projectId:'saved',revision:7,document,activeCompositionId:document.root,timeUs:100000});vi.mocked(api.getNestedCompositionProject).mockResolvedValue({id:'saved',name:'Saved',revision:7,document} as api.NestedCompositionProjectDto);return document.root}
it('uses saved identity and preserves project/history for analysis and preview',async()=>{
 const timeline=setup(),report={sourceId:'raw',analysedFrames:4,analysedRuns:1,emptyRuns:0,cacheReused:false},frame={width:1,height:1,rgba:new Uint8ClampedArray([0,255,0,255])}
 vi.mocked(api.analyseNestedVideoCutout).mockResolvedValue(report);vi.mocked(api.previewNestedVideoCutout).mockResolvedValue(frame)
 expect(await analyseNestedSavedVideoCutout('token',timeline,'clip')).toEqual(report);expect(api.analyseNestedVideoCutout).toHaveBeenCalledWith('saved',timeline,'clip',7,'token',undefined,'person')
 expect(await previewNestedSavedVideoCutout('token',timeline,'clip',100000)).toEqual(frame);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.revision).toBe(7)
})
it('refuses unsaved local edits before model work',async()=>{const timeline=setup();const doc=nestedComposition.document!,node=doc.compositions[timeline]!;nestedComposition.document={...doc,compositions:{...doc.compositions,[timeline]:{...node,content:{...node.content,canvas:{...node.content.canvas,width:node.content.canvas.width+2}}}}};expect(await analyseNestedSavedVideoCutout('token',timeline,'clip')).toBeNull();expect(api.analyseNestedVideoCutout).not.toHaveBeenCalled();expect(nestedHistory.error).toContain('Сначала сохраните')})
it('discards late frames after cursor changes and refuses pre-cancellation',async()=>{const timeline=setup();let finish!:(frame:api.NativeNestedPreviewFrame)=>void;vi.mocked(api.previewNestedVideoCutout).mockReturnValue(new Promise(resolve=>{finish=resolve}));const pending=previewNestedSavedVideoCutout('token',timeline,'clip',100000);await vi.waitFor(()=>expect(api.previewNestedVideoCutout).toHaveBeenCalled());nestedComposition.timeUs=200000;finish({width:1,height:1,rgba:new Uint8ClampedArray(4)});expect(await pending).toBeNull();const cancel=new AbortController();cancel.abort();expect(await analyseNestedSavedVideoCutout('token',timeline,'clip',cancel.signal)).toBeNull()})

function updatedBinding(timeline:string,enabled:boolean,revision:number):api.NestedCompositionProjectDto{
 const document=JSON.parse(JSON.stringify(nestedComposition.document)) as api.NativeNestedProject,node=document.compositions[timeline]!
 const binding={subject:'person' as const,sourceHash:'a'.repeat(64),modelHash:'88d4531297118f595bf2fd60f6f566aec2e559393802d1f436c380f0cbbd2828',runtimeHash:'b'.repeat(64)}
 return {id:'saved',name:'Saved',revision,document:{...document,compositions:{...document.compositions,[timeline]:{...node,content:{...node.content,tracks:node.content.tracks.map(track=>track.kind==='video'?{...track,clips:track.clips.map(clip=>clip.id==='picture'?{...clip,concatCutout:enabled?binding:undefined}:clip)}:track)}}}}} as api.NestedCompositionProjectDto
}
it('commits enable/disable as revisioned history and preserves undo/redo',async()=>{
 const timeline=setup(),enabled=updatedBinding(timeline,true,8);vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(enabled)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true)).toBe(true);expect(api.setNestedVideoCutoutBinding).toHaveBeenCalledWith('saved',timeline,'picture',7,true,'token',undefined);expect(nestedComposition.revision).toBe(8);expect(nestedHistory.past).toHaveLength(1)
 undoNestedComposition();expect(nestedComposition.revision).toBe(8);expect(nestedComposition.document).not.toEqual(enabled.document);redoNestedComposition();expect(nestedComposition.document).toEqual(enabled.document)
 vi.mocked(api.getNestedCompositionProject).mockResolvedValue(enabled);const disabled=updatedBinding(timeline,false,9);vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(disabled)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',false)).toBe(true);expect(nestedComposition.revision).toBe(9);expect(nestedHistory.past).toHaveLength(2)
})
it('rejects activation responses for a different project and stale selection',async()=>{
 const timeline=setup(),updated=updatedBinding(timeline,true,8);vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue({...updated,id:'other'})
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true)).toBe(false);expect(nestedHistory.past).toHaveLength(0)
 setup();let finish!:(value:api.NestedCompositionProjectDto)=>void;vi.mocked(api.setNestedVideoCutoutBinding).mockReturnValue(new Promise(resolve=>{finish=resolve}));const pending=setNestedSavedVideoCutout('token',timeline,'picture',true);await vi.waitFor(()=>expect(api.setNestedVideoCutoutBinding).toHaveBeenCalledTimes(2));nestedComposition.projectId='other';finish(updated);expect(await pending).toBe(false);expect(nestedHistory.past).toHaveLength(0)
})

it('passes the selected video subject without changing the saved document',async()=>{
 const timeline=setup(),document=nestedComposition.document,report={sourceId:'raw',analysedFrames:2,analysedRuns:1,emptyRuns:0,cacheReused:false}
 vi.mocked(api.analyseNestedVideoCutout).mockResolvedValue(report)
 for(const subject of ['auto','object','person'] as const){expect(await analyseNestedSavedVideoCutout('token',timeline,'clip',undefined,subject)).toEqual(report);expect(api.analyseNestedVideoCutout).toHaveBeenLastCalledWith('saved',timeline,'clip',7,'token',undefined,subject)}
 expect(nestedComposition.document).toEqual(document);expect(nestedHistory.past).toHaveLength(0)
})

it('persists Auto intent on activation and refuses an unconfirmed subject',async()=>{
 const timeline=setup(),updated=updatedBinding(timeline,true,8)
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)(clip as {concatCutout:{subject:string}}).concatCutout.subject='auto'
 vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updated)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,'auto')).toBe(true)
 expect(api.setNestedVideoCutoutBinding).toHaveBeenLastCalledWith('saved',timeline,'picture',7,true,'token',undefined,'auto')
 expect(nestedComposition.document).toEqual(updated.document)
 closeNestedComposition();setup();vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updatedBinding(timeline,true,8))
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,'auto')).toBe(false)
 expect(nestedHistory.past).toHaveLength(0)
})

it('confirms the requested model before committing a reapplication',async()=>{
 const timeline=setup(),updated=updatedBinding(timeline,true,8),expected='60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a'
 vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updated)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,'person',expected)).toBe(false);expect(nestedHistory.past).toHaveLength(0);expect(api.setNestedVideoCutoutBinding).toHaveBeenLastCalledWith('saved',timeline,'picture',7,true,'token',undefined,'person',expected)
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)Object.assign(clip.concatCutout,{subject:'auto',modelHash:expected})
 vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updated)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,'auto',expected)).toBe(true);expect(nestedHistory.past).toHaveLength(1)
 undoNestedComposition();expect(nestedComposition.document).not.toEqual(updated.document);redoNestedComposition();expect(nestedComposition.document).toEqual(updated.document)
})

it('keeps video edge softness in preview and revisioned history and verifies server confirmation',async()=>{
 const timeline=setup(),frame={width:1,height:1,rgba:new Uint8ClampedArray(4)};vi.mocked(api.previewNestedVideoCutout).mockResolvedValue(frame);expect(await previewNestedSavedVideoCutout('token',timeline,'picture',100000,undefined,0.025)).toEqual(frame);expect(api.previewNestedVideoCutout).toHaveBeenLastCalledWith('saved',timeline,'picture',7,100000,'token',undefined,0.025)
 const updated=updatedBinding(timeline,true,8);vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updated);expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,'person',undefined,0.025)).toBe(false);expect(nestedHistory.past).toHaveLength(0)
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)Object.assign(clip.concatCutout,{feather:0.025})
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,'person',undefined,0.025)).toBe(true);expect(api.setNestedVideoCutoutBinding).toHaveBeenLastCalledWith('saved',timeline,'picture',7,true,'token',undefined,'person',undefined,0.025);expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).not.toEqual(updated.document);redoNestedComposition();expect(nestedComposition.document).toEqual(updated.document)
})

it('confirms authored strokes semantically and preserves them through undo/redo',async()=>{
 const timeline=setup(),correction:api.MaskCorrection={custom:true,feather:0.01,strokes:[{tool:'brush',size:0.1,points:[[0.25,0.5]]}]},updated=updatedBinding(timeline,true,8)
 vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updated)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,undefined,undefined,undefined,correction)).toBe(false);expect(nestedHistory.past).toHaveLength(0)
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)Object.assign(clip.concatCutout,{correction:{feather:0.01,strokes:[{points:[[0.25,0.5]],size:0.1,tool:'brush',at:null}],custom:true}})
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,undefined,undefined,undefined,correction)).toBe(true)
 expect(api.setNestedVideoCutoutBinding).toHaveBeenLastCalledWith('saved',timeline,'picture',7,true,'token',undefined,undefined,undefined,undefined,correction)
 expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).not.toEqual(updated.document);redoNestedComposition();expect(nestedComposition.document).toEqual(updated.document)
})

it('requires explicit temporal track removal to be confirmed before recording history',async()=>{
 const timeline=setup(),correction:api.MaskCorrection={custom:false,feather:0,strokes:[]},updated=updatedBinding(timeline,true,8)
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)Object.assign(clip.concatCutout,{feather:0,correction,regionTracks:[{stroke:0,snapshot:{maskSourceId:'not-removed'}}]})
 vi.mocked(api.setNestedVideoCutoutBinding).mockResolvedValue(updated)
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,undefined,undefined,0,correction,[])).toBe(false);expect(nestedHistory.past).toHaveLength(0)
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)Object.assign(clip.concatCutout,{regionTracks:[]})
 expect(await setNestedSavedVideoCutout('token',timeline,'picture',true,undefined,undefined,undefined,0,correction,[])).toBe(true);expect(api.setNestedVideoCutoutBinding).toHaveBeenLastCalledWith('saved',timeline,'picture',7,true,'token',undefined,undefined,undefined,0,correction,[])
 expect(nestedHistory.past).toHaveLength(1);undoNestedComposition();expect(nestedComposition.document).not.toEqual(updated.document);redoNestedComposition();expect(nestedComposition.document).toEqual(updated.document)
})

it('starts a smart job against saved scope and refuses an inconsistent source clock',async()=>{
 const timeline=setup(),points:[number,number][]=[[.5,.5]],accepted={jobId:'job',sourceId:'source',sourceUs:100000,seedFrame:1,frames:3,frameRate:{numerator:10,denominator:1},expectedRevision:7};vi.mocked(api.startNestedVideoSmartRegion).mockResolvedValue(accepted)
 expect(await startNestedSavedVideoSmartRegion('token',timeline,'picture',points)).toEqual(accepted);expect(api.startNestedVideoSmartRegion).toHaveBeenLastCalledWith('saved',timeline,'picture',7,100000,points,'token',undefined);expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.revision).toBe(7)
 for(const wrong of [{...accepted,sourceId:'other'},{...accepted,seedFrame:0},{...accepted,expectedRevision:8}]){vi.mocked(api.startNestedVideoSmartRegion).mockResolvedValue(wrong);expect(await startNestedSavedVideoSmartRegion('token',timeline,'picture',points)).toBeNull();expect(nestedHistory.past).toHaveLength(0)}
})

function completedSmartAttachment(vfr=false){
 const timeline=setup(),initial=updatedBinding(timeline,true,7);nestedComposition.document=initial.document;vi.mocked(api.getNestedCompositionProject).mockResolvedValue(initial)
 const updated=JSON.parse(JSON.stringify(initial)) as api.NestedCompositionProjectDto;updated.revision=8;const points:[number,number][]=[[.5,.5]],started={...(vfr?{frameTimesUs:[0,100000,400000,500000]}:{}),jobId:'smart-job',sourceId:'source',sourceUs:vfr?250000:100000,seedFrame:1,frames:3,frameRate:{numerator:10,denominator:1},expectedRevision:7}
 const region={sourceId:'source',maskSourceId:'region',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'0'.repeat(64),points,width:2,height:2,frames:3,frameRate:started.frameRate,...(vfr?{frameTimesUs:started.frameTimesUs}:{}),seedFrame:1,trackedFrames:3,forwardLostAt:null,backwardLostAt:null}
 for(const track of updated.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout)Object.assign(clip.concatCutout,{correction:{custom:true,feather:0,strokes:[{tool:'smartBrush',size:.1,points,at:vfr?.25:.1}]},regionTracks:[{stroke:0,snapshot:region}]})
 ;(updated.document.compositions[timeline]!.content.sources as Record<string,unknown>).region={id:'region',kind:'video',durationTicks:300000,width:256,height:256,hasAudio:false}
 return {timeline,initial,updated,points,started}
}
it('commits a completed SAM stroke once and preserves the region through undo/redo',async()=>{
 const {timeline,initial,updated,points,started}=completedSmartAttachment();vi.mocked(api.attachNestedVideoSmartRegion).mockResolvedValue(updated)
 expect(await attachNestedSavedVideoSmartRegion('token',timeline,'picture',100000,started,points,'smartBrush',.1)).toBe(true);expect(api.attachNestedVideoSmartRegion).toHaveBeenLastCalledWith('saved',timeline,'picture',7,100000,'smart-job','smartBrush',.1,'token',undefined);expect(nestedHistory.past).toHaveLength(1);expect(nestedComposition.revision).toBe(8)
 undoNestedComposition();expect(nestedComposition.document).toEqual(initial.document);expect(nestedComposition.revision).toBe(8);redoNestedComposition();expect(nestedComposition.document).toEqual(updated.document)
})
it('refuses changed smart results and late completion after cursor movement',async()=>{
 const {timeline,updated,points,started}=completedSmartAttachment();const wrong=JSON.parse(JSON.stringify(updated)) as api.NestedCompositionProjectDto
 for(const track of wrong.document.compositions[timeline]!.content.tracks)if(track.kind==='video')for(const clip of track.clips)if(clip.concatCutout?.regionTracks)(clip.concatCutout.regionTracks[0]!.snapshot as {seedFrame:number}).seedFrame=2
 vi.mocked(api.attachNestedVideoSmartRegion).mockResolvedValue(wrong);expect(await attachNestedSavedVideoSmartRegion('token',timeline,'picture',100000,started,points,'smartBrush',.1)).toBe(false);expect(nestedHistory.past).toHaveLength(0)
 let finish!:(value:api.NestedCompositionProjectDto)=>void;vi.mocked(api.attachNestedVideoSmartRegion).mockReturnValue(new Promise(resolve=>{finish=resolve}));const pending=attachNestedSavedVideoSmartRegion('token',timeline,'picture',100000,started,points,'smartBrush',.1);await vi.waitFor(()=>expect(api.attachNestedVideoSmartRegion).toHaveBeenCalledTimes(2));nestedComposition.timeUs=200000;finish(updated);expect(await pending).toBe(false);expect(nestedHistory.past).toHaveLength(0)
})


for(const vfr of [false,true])it(`recognizes a committed smart append after lost response with VFR ${vfr}`, async()=>{
 const {captureSmartVideoBase}=await import('../composition/smartVideoReceipt')
 const {reconcileNestedSavedVideoSmartRegion}=await import('./nestedComposition.svelte')
 const {timeline,initial,updated,points,started}=completedSmartAttachment(vfr)
 nestedComposition.name=initial.name
 const base=await captureSmartVideoBase(initial.document,timeline,'picture')
 const intent={version:2 as const,id:'intent',projectId:'saved',compositionId:timeline,clipId:'picture',sourceHash:'a'.repeat(64),timeUs:100000,points,tool:'smartBrush' as const,size:.1,started,base}
 const job={id:started.jobId,status:'done',result:{...(vfr?{frameTimesUs:started.frameTimesUs}:{}),sourceHash:'a'.repeat(64),seedFrame:1,frames:3,width:2,height:2,frameRate:started.frameRate,tracking:{trackedFrames:3,forwardLostAt:null,backwardLostAt:null,outputHash:'b'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64)},regionVideo:{fileHash:'c'.repeat(64)},seedRegion:{promptHash:'0'.repeat(64)}}} as import('../types').Job
 vi.mocked(api.getNestedCompositionProject).mockResolvedValue(updated)
 const forged=JSON.parse(JSON.stringify(job));forged.result.tracking.outputHash='9'.repeat(64)
 expect(await reconcileNestedSavedVideoSmartRegion('token',intent,forged)).toBe('blocked')
 expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(initial.document)
 const changed=JSON.parse(JSON.stringify(updated));changed.document.compositions[timeline].content.canvas.width+=1
 vi.mocked(api.getNestedCompositionProject).mockResolvedValue(changed)
 expect(await reconcileNestedSavedVideoSmartRegion('token',intent,job)).toBe('blocked')
 expect(nestedHistory.past).toHaveLength(0)
 vi.mocked(api.getNestedCompositionProject).mockResolvedValue(updated)
 expect(await reconcileNestedSavedVideoSmartRegion('token',intent,job)).toBe('applied')
 expect(nestedComposition.document).toEqual(updated.document);expect(nestedHistory.past).toHaveLength(1)
 expect(await reconcileNestedSavedVideoSmartRegion('token',intent,job)).toBe('applied')
 expect(nestedHistory.past).toHaveLength(1);expect(api.attachNestedVideoSmartRegion).not.toHaveBeenCalled()
 undoNestedComposition();expect(nestedComposition.document).toEqual(initial.document)
})
