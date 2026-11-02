import {setNativeSourceSpeedCurve,validateSourceSpeedPoints,type SourceSpeedPoints} from '../composition/concatSourceCurve'
import {slipNativeLayer} from '../composition/nativeSlip'
import {raiseNativeLayers} from '../composition/nativeRaise'
import {pasteNativeLayers} from '../composition/nativePaste'
import {rippleNativeComposition} from '../composition/nativeRipple'
import {upsertCompositionMarker,deleteCompositionMarker,type CompositionMarker} from '../composition/markers'
import {NATIVE_PATH_OPERATIONS,applyNativePathOperation} from './nativePath'
import {NATIVE_COLOR_OPERATIONS,applyNativeColorOperation} from './nativeColor'
import {NATIVE_KEYFRAME_OPERATIONS,applyNativeKeyframeOperation,setNativeVisualConstant} from './nativeKeyframes'
import {rebindNativeLayerReferences} from '../composition/nativeLayerReferences'
import {moveNativeNestedClips,type NativeNestedProject} from '../api'
import {createComposition} from '../composition/commands'
import {buildCompositionWireDocument} from '../composition/payload'
import {patchCompositionCanvas} from '../composition/canvas'
import {editNativeTrack} from '../composition/nativeTrackEdit'
import {removeNativeClips} from '../composition/nativeClipDelete'
import {nativeClipStart} from '../composition/nativeClipMove'
import {nativeClipEnd} from '../composition/nativeClipInsert'
import {trimNativeClipsWithSourcePlans} from '../composition/concatSourceTrim'
import {splitNativeClipsWithSourcePlans} from '../composition/concatSourceSplit'
import {duplicateNativeClipsWithSourcePlans} from '../composition/concatSourceCopy'
import {prepareNativeLinkEscape,expandNativeClipLinks,linkNativeClips,unlinkNativeClips} from '../composition/nativeClipLinks'
import {isStableId,MAX_AUDIO_GAIN} from '../composition/validation'
import {COMPOSITION_BLEND_MODES} from '../composition/types'
import type {Composition,CompositionTrack,WireCompositionTrack,WireTextStyle} from '../composition/types'
import {evaluateAgentBatch,type AgentOperation} from './operations'
export const NATIVE_BATCH_OPERATIONS=['slip_layer','move_layers_to_new_track','paste_layers','ripple_delete_gap','ripple_delete_layers','add_marker','remove_marker','add_track','update_track','delete_track','reorder_track','add_layer','delete_layer','move_layer','move_layers','move_layer_placements','trim_layer','set_speed_curve','split_layer','duplicate_layer','set_layers_enabled','update_layer','update_canvas','add_transition','remove_transition','create_link','remove_link',...NATIVE_KEYFRAME_OPERATIONS,...NATIVE_COLOR_OPERATIONS,...NATIVE_PATH_OPERATIONS] as const
export const NATIVE_LAYER_PATCH_FIELDS=['text','enabled','opacity','x','y','scaleX','scaleY','rotationDegrees','gain','pan','audioGain','audioPan','sourceAudioEnabled','blendMode','style','concatTitle','effects'] as const
export const NATIVE_TEXT_STYLE_PATCH_FIELDS=['fontFamily','fontSizePx','color','backgroundColor','strokeColor','strokeWidthPx','shadowColor','shadowX','shadowY','align'] as const
export const NATIVE_CONCAT_TITLE_PATCH_FIELDS=['fontFamily','fontSize','fontWeight','italic','color','align','opacity','strokeWidth','strokeColor','shadow','background','backgroundRadius','backgroundPaddingX','backgroundPaddingY','lineHeight','tracking','maxWidth','maxHeight'] as const
type Row=Record<string,unknown>
function object(value:unknown):Row {if(!value||typeof value!=='object'||Array.isArray(value))throw new Error('Expected an operation object');return value as Row}
function id(value:unknown):string {if(!isStableId(value))throw new Error('Invalid stable ID');return value as string}
function integer(value:unknown):number {if(!Number.isSafeInteger(value)||Number(value)<0)throw new Error('Expected nonnegative integer microseconds/index');return value as number}
function ids(value:unknown):string[]{if(!Array.isArray(value)||!value.length||Array.from(value).length>512||new Set(value).size!==value.length)throw new Error('Expected 1–512 distinct layer IDs');return value.map(id)}
function fields(op:Row,names:readonly string[]){if(Object.keys(op).some(key=>key!=='op'&&!names.includes(key)))throw new Error(`Unsupported native ${String(op.op)} argument`)}
function tracks(project:NativeNestedProject,scope:string,update:(tracks:readonly WireCompositionTrack[])=>readonly WireCompositionTrack[]):NativeNestedProject{
 const node=project.compositions[scope]!
 return {...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks:rebindNativeLayerReferences(node.content.tracks,update(node.content.tracks))}}}}
}
function locate(project:NativeNestedProject,scope:string,clipId:string){
 const track=project.compositions[scope]!.content.tracks.find(track=>track.clips.some(clip=>clip.id===clipId))
 if(!track)throw new Error('Layer not found in composition')
 return {track,clip:track.clips.find(clip=>clip.id===clipId)!}
}
function seed(project:NativeNestedProject,scope:string):Composition {
 const content=project.compositions[scope]!.content,c=content.canvas
 return createComposition({width:c.width,height:c.height,fps:c.fpsRate?c.fpsRate.numerator/c.fpsRate.denominator:c.fpsMilli/1000,...(c.fpsRate?{fpsRate:c.fpsRate}:{}),backgroundColor:'#00000000'},content.sources)
}
function authoredTrack(project:NativeNestedProject,scope:string,track:unknown):WireCompositionTrack {
 const authored=object(track)
 if(Object.keys(authored).some(key=>!['id','kind','name','locked','hidden','muted','solo','clips','transitions'].includes(key)))throw new Error('Unsupported native track field')
 if(Array.isArray(authored.clips)&&authored.clips.some(clip=>object(clip).locked===true))throw new Error('Native layers use track locks; per-layer locks cannot be inserted')
 const document=evaluateAgentBatch(seed(project,scope),[{op:'add_track',track:track as CompositionTrack}])
 return buildCompositionWireDocument(document).tracks[0]!
}
function patchTextStyle(project:NativeNestedProject,scope:string,current:WireTextStyle,value:unknown):WireTextStyle {
 const patch=object(value),keys=Object.keys(patch)
 if(!keys.length||keys.some(key=>!(NATIVE_TEXT_STYLE_PATCH_FIELDS as readonly string[]).includes(key)))throw new Error('Unsupported native text style field')
 const document=evaluateAgentBatch(seed(project,scope),[{op:'add_track',track:{id:'style-track',kind:'text',name:'Style',hidden:false,locked:false,clips:[]}},{op:'add_layer',trackId:'style-track',clip:{id:'style-clip',kind:'text',text:'Style',timelineStartTicks:0,durationTicks:1,x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center',...patch}}}])
 const clip=buildCompositionWireDocument(document).tracks[0]!.clips[0]!
 if(!('style' in clip))throw new Error('Text style conversion failed')
 const names:Record<string,keyof WireTextStyle>={fontFamily:'fontFamily',fontSizePx:'fontSize',color:'color',backgroundColor:'background',strokeColor:'stroke',strokeWidthPx:'strokeWidth',shadowColor:'shadow',shadowX:'shadowX',shadowY:'shadowY',align:'align'}
 return {...current,...Object.fromEntries(keys.map(key=>[names[key]!,clip.style[names[key]!]]))}
}
function allocateIds(project:NativeNestedProject,scope:string,clipId:string,requested:string,at?:number):()=>string {
 const selected=new Set(expandNativeClipLinks(project,scope,[clipId]))
 let serial=0
 const queue=project.compositions[scope]!.content.tracks.flatMap(track=>track.clips.filter(clip=>selected.has(clip.id)&&(at===undefined||nativeClipStart(clip)<at&&at<nativeClipEnd(clip))).map(clip=>clip.id===clipId?requested:`${requested.slice(0,100)}-linked-${serial++}`))
 return ()=>queue.shift()??`${requested.slice(0,100)}-aux-${serial++}`
}
/** Evaluate only a private candidate. Retained wire content is never round-tripped through authoring state. */
export async function evaluateNativeAgentBatch(input:NativeNestedProject,scope:string,value:unknown):Promise<NativeNestedProject>{
 if(!Object.hasOwn(input.compositions,scope))throw new Error('Composition not found')
 if(!Array.isArray(value)||!value.length||value.length>100)throw new Error('Batch requires 1–100 operations')
 let project=input
 const operations=JSON.parse(JSON.stringify(value)) as unknown[]
 for(const raw of operations){
  const op=object(raw),name=op.op
  switch(name){
   case 'slip_layer':fields(op,['clipId','sourceDeltaTicks']);project=slipNativeLayer(project,scope,id(op.clipId),op.sourceDeltaTicks as number);break

   case 'move_layers_to_new_track':{
    fields(op,['clipIds','trackId','anchor'])
    const anchor=op.anchor===undefined?undefined:object(op.anchor)
    if(anchor&&Object.keys(anchor).some(field=>!['clipId','startTicks'].includes(field)))throw new Error('Invalid raise anchor field')
    project=await raiseNativeLayers(project,scope,ids(op.clipIds),id(op.trackId),anchor?{clipId:id(anchor.clipId),startTicks:integer(anchor.startTicks)}:undefined);break
   }

   case 'paste_layers':{
    fields(op,['subjects','trackId','startTicks','linkIds'])
    project=await pasteNativeLayers(project,scope,op.subjects as {clipId:string;id:string}[],{...(op.trackId===undefined?{}:{trackId:id(op.trackId)}),...(op.startTicks===undefined?{}:{startTicks:integer(op.startTicks)}),...(op.linkIds===undefined?{}:{linkIds:op.linkIds as string[]})});break
   }

   case 'ripple_delete_gap':fields(op,['trackId','startTicks','endTicks']);project=await rippleNativeComposition(project,scope,{trackId:id(op.trackId),start:integer(op.startTicks),end:integer(op.endTicks)});break
   case 'ripple_delete_layers':fields(op,['clipIds']);project=await rippleNativeComposition(project,scope,{clipIds:ids(op.clipIds)});break

   case 'add_marker':case 'remove_marker':{
    fields(op,name==='add_marker'?['marker']:['markerId'])
    const node=project.compositions[scope]!,metadata=node.content.metadata
    const authored={...seed(project,scope),markers:metadata?.markers??[]}
    const changed=name==='add_marker'?upsertCompositionMarker(authored,op.marker as CompositionMarker):deleteCompositionMarker(authored,id(op.markerId))
    const markers=(changed as typeof authored).markers
    project={...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,metadata:{...metadata,markers}}}}};break
   }

   case 'add_track':{
    fields(op,['track','index']);const track=authoredTrack(project,scope,op.track)
    const node=project.compositions[scope]!
    if([...node.content.tracks,...(node.content.captionTracks??[])].some(item=>item.id===track.id))throw new Error('Track ID already exists')
    const index=op.index===undefined?node.content.tracks.length:integer(op.index)
    if(index>node.content.tracks.length)throw new Error('Track index out of bounds')
    project=tracks(project,scope,rows=>[...rows.slice(0,index),track,...rows.slice(index)]);break
   }
   case 'update_track':{
    fields(op,['trackId','patch']);const trackId=id(op.trackId),patch=object(op.patch)
    if(!Object.keys(patch).length||Object.keys(patch).some(key=>!['name','hidden','muted','solo','locked'].includes(key)))throw new Error('Invalid native track patch')
    if(patch.locked===false)project=editNativeTrack(project,scope,{kind:'flag',id:trackId,flag:'locked',value:false})
    for(const [key,value] of Object.entries(patch).filter(([key])=>key!=='locked')){
     if(key==='name'){if(typeof value!=='string')throw new Error('Track name must be text');project=editNativeTrack(project,scope,{kind:'rename',id:trackId,name:value})}
     else {if(typeof value!=='boolean')throw new Error('Track flags must be boolean');project=editNativeTrack(project,scope,{kind:'flag',id:trackId,flag:key as 'hidden'|'muted'|'solo'|'locked',value})}
    }
    if(Object.hasOwn(patch,'locked')){if(typeof patch.locked!=='boolean')throw new Error('Track flags must be boolean');project=editNativeTrack(project,scope,{kind:'flag',id:trackId,flag:'locked',value:patch.locked})}
    break
   }
   case 'delete_track':fields(op,['trackId']);project=editNativeTrack(project,scope,{kind:'remove',id:id(op.trackId),removeClips:true});break
   case 'reorder_track':{
    fields(op,['trackId','index']);const trackId=id(op.trackId),index=integer(op.index),rows=[...project.compositions[scope]!.content.tracks],old=rows.findIndex(track=>track.id===trackId)
    if(old<0||index>=rows.length)throw new Error('Track/index not found')
    const [track]=rows.splice(old,1);rows.splice(index,0,track!);project=tracks(project,scope,()=>rows);break
   }
   case 'add_layer':{
    fields(op,['trackId','clip']);const trackId=id(op.trackId),target=project.compositions[scope]!.content.tracks.find(track=>track.id===trackId)
    if(!target||target.locked)throw new Error('Target track missing or locked')
    const clip=object(op.clip)
    const converted=authoredTrack(project,scope,{...target,clips:[],...(target.kind==='video'?{transitions:[]}:{} )})
    const doc=evaluateAgentBatch({...seed(project,scope),tracks:[{id:converted.id,kind:converted.kind,name:converted.name,locked:false,hidden:false,muted:false,clips:[]} as CompositionTrack]},[{op:'add_layer',trackId,clip:op.clip} as AgentOperation])
    if(clip.locked===true)throw new Error('Native layers use track locks')
    const wire=buildCompositionWireDocument(doc).tracks[0]!.clips[0]!
    if([...project.compositions[scope]!.content.tracks,...(project.compositions[scope]!.content.captionTracks??[])].some(track=>track.clips.some(item=>item.id===wire.id)))throw new Error('Layer ID already exists')
    project=tracks(project,scope,rows=>rows.map(track=>track.id===trackId?{...track,clips:[...track.clips,wire]} as WireCompositionTrack:track));break
   }
   case 'delete_layer':fields(op,['clipId']);project=removeNativeClips(project,scope,[id(op.clipId)]);break
   case 'move_layer':case 'move_layers':case 'move_layer_placements':{
    fields(op,name==='move_layer_placements'?['placements','anchorId','startTicks']:name==='move_layer'?['clipId','trackId','startTicks','escapeLink']:['clipIds','trackId','startTicks'])
    if(op.escapeLink!==undefined&&typeof op.escapeLink!=='boolean')throw new Error('escapeLink must be boolean')
    let requested:string[],target:string,placementTargets:Map<string,string>|undefined
    if(name==='move_layer_placements'){
     if(!Array.isArray(op.placements)||!op.placements.length||op.placements.length>512)throw new Error('Placements require 1–512 subjects')
     const placements=op.placements.map(value=>{const row=object(value);if(Object.keys(row).some(field=>!['clipId','trackId'].includes(field)))throw new Error('Invalid placement field');return {clipId:id(row.clipId),trackId:id(row.trackId)}})
     const selected=ids(placements.map(row=>row.clipId)),anchorId=id(op.anchorId)
     if(!selected.includes(anchorId))throw new Error('Placement anchor must be selected')
     requested=[anchorId,...selected.filter(id=>id!==anchorId)];placementTargets=new Map(placements.map(row=>[row.clipId,row.trackId]));target=placementTargets.get(anchorId)!
    }else{requested=name==='move_layer'?[id(op.clipId)]:ids(op.clipIds);target=id(op.trackId)}
    const anchor=requested[0]!
    const selected=name==='move_layer'&&op.escapeLink!==true?[anchor,...expandNativeClipLinks(project,scope,requested).filter(id=>id!==anchor)]:requested
    if(selected.length>512)throw new Error('Move requires at most 512 clips')
    const escape=op.escapeLink===true||name!=='move_layer'?prepareNativeLinkEscape(project,scope,selected):undefined
    if(escape)project=escape.project
    const locations=selected.map(clipId=>locate(project,scope,clipId)),origin=Math.min(...locations.map(row=>nativeClipStart(row.clip)))
    const delta=Math.max(-origin,integer(op.startTicks)-nativeClipStart(locations[0]!.clip))
    const moves=locations.map((row,index)=>{
     const toTrackId=placementTargets?.get(row.clip.id)??(index===0?target:row.track.id),to=project.compositions[scope]!.content.tracks.find(track=>track.id===toTrackId)
     if(row.track.locked||!to||to.locked||to.kind!==row.track.kind)throw new Error('Move source/target is locked, missing or has a different kind')
     const startUs=nativeClipStart(row.clip)+delta
     if(!Number.isSafeInteger(startUs)||startUs<0)throw new Error('Move exceeds the exact timeline range')
     return {fromComposition:scope,fromTrackId:row.track.id,clipId:row.clip.id,toComposition:scope,toTrackId,startUs,createTrack:false}
    })
    const result=await moveNativeNestedClips(project,moves)
    // Native serde may normalize other nodes: retain all unrelated document content verbatim.
    project={...project,compositions:{...project.compositions,[scope]:result.project.compositions[scope]!}};if(escape)project=escape.restore(project);break
   }
   case 'trim_layer':{
    fields(op,['clipId','startTicks','endTicks','escapeLink']);if(op.escapeLink!==undefined&&typeof op.escapeLink!=='boolean')throw new Error('escapeLink must be boolean')
    const clipId=id(op.clipId),start=integer(op.startTicks),end=integer(op.endTicks),current=locate(project,scope,clipId).clip
    const escape=op.escapeLink===true?prepareNativeLinkEscape(project,scope,[clipId]):undefined
    if(escape)project=escape.project
    if(end<=start)throw new Error('Invalid trim range')
    if(start>=nativeClipEnd(current))project=await trimNativeClipsWithSourcePlans(project,scope,[clipId],end-nativeClipEnd(current),'end')
    project=await trimNativeClipsWithSourcePlans(project,scope,[clipId],start-nativeClipStart(current),'start')
    project=await trimNativeClipsWithSourcePlans(project,scope,[clipId],end-nativeClipEnd(locate(project,scope,clipId).clip),'end');if(escape)project=escape.restore(project);break
   }
   case 'set_speed_curve':{
    fields(op,['clipIds','points'])
    if(!Object.hasOwn(op,'points'))throw new Error('Speed curve points or null are required')
    validateSourceSpeedPoints(op.points as SourceSpeedPoints)
    project=await setNativeSourceSpeedCurve(project,scope,ids(op.clipIds),op.points as SourceSpeedPoints);break
   }
   case 'split_layer':{
    fields(op,['clipId','atTicks','rightClipId','escapeLink']);if(op.escapeLink!==undefined&&typeof op.escapeLink!=='boolean')throw new Error('escapeLink must be boolean')
    const clipId=id(op.clipId),at=integer(op.atTicks),right=id(op.rightClipId),clip=locate(project,scope,clipId).clip
    if(at<=nativeClipStart(clip)||at>=nativeClipEnd(clip))throw new Error('Split must fall inside the layer')
    const escape=op.escapeLink===true?prepareNativeLinkEscape(project,scope,[clipId]):undefined
    if(escape)project=escape.project
    project=await splitNativeClipsWithSourcePlans(project,scope,[clipId],at,allocateIds(project,scope,clipId,right,at));if(escape)project=escape.restore(project);break
   }
   case 'duplicate_layer':{
    fields(op,['clipId','id','startTicks','trackId','escapeLink','linkId']);const clipId=id(op.clipId),copy=id(op.id),original=locate(project,scope,clipId)
    if(op.escapeLink!==undefined&&typeof op.escapeLink!=='boolean')throw new Error('escapeLink must be boolean')
    if(op.linkId!==undefined)id(op.linkId)
    if(op.escapeLink===true||op.linkId!==undefined||op.trackId!==undefined&&op.trackId!==original.track.id){
     const selected=op.escapeLink===true?[clipId]:expandNativeClipLinks(project,scope,[clipId])
     const mint=allocateIds(project,scope,clipId,copy),copies=new Map(project.compositions[scope]!.content.tracks.flatMap(track=>track.clips.filter(clip=>selected.includes(clip.id)).map(clip=>[clip.id,op.escapeLink===true?copy:mint()] as const)))
     const ordered=[clipId,...selected.filter(id=>id!==clipId)]
     project=await pasteNativeLayers(project,scope,ordered.map(clipId=>({clipId,id:copies.get(clipId)!})),{trackId:op.trackId===undefined?original.track.id:id(op.trackId),startTicks:op.startTicks===undefined?nativeClipEnd(original.clip):integer(op.startTicks),...(selected.length>1?{linkIds:[op.linkId===undefined?`${copy}-link`:id(op.linkId)]}:{})});break
    }
    project=await duplicateNativeClipsWithSourcePlans(project,scope,[clipId],allocateIds(project,scope,clipId,copy),op.startTicks===undefined?undefined:integer(op.startTicks));break
   }
   case 'set_layers_enabled':case 'update_layer':{
    fields(op,name==='update_layer'?['clipId','patch']:['clipIds','enabled'])
    const selected=name==='update_layer'?[id(op.clipId)]:ids(op.clipIds),patch=name==='update_layer'?object(op.patch):{enabled:op.enabled}
    if(!Object.keys(patch).length||Object.keys(patch).some(key=>!(NATIVE_LAYER_PATCH_FIELDS as readonly string[]).includes(key)))throw new Error('Unsupported native layer patch; read nativeBatch capabilities')
    for(const clipId of selected){const {track}=locate(project,scope,clipId);if(track.locked)throw new Error('Track is locked')}
    project=tracks(project,scope,rows=>rows.map(track=>!track.clips.some(clip=>selected.includes(clip.id))?track:({...track,clips:track.clips.map(clip=>{
     if(!selected.includes(clip.id))return clip
     const next={...clip} as typeof clip
     for(const [key,value] of Object.entries(patch)){
      if(key==='text'){if(!('text' in next)||typeof value!=='string'||!value.length||Array.from(value).length>512||value.includes('\0'))throw new Error('Invalid text layer patch');Object.assign(next,{text:value})}
      else if(key==='effects'){if((!('effects' in next)&&!('concatTitle' in next&&next.concatTitle)&&!Object.hasOwn(patch,'concatTitle'))||!Array.isArray(value)||value.some(effect=>typeof object(effect).kind!=='string'))throw new Error('Effects require a visual layer and native effect objects');if(track.kind==='text'&&!value.length)delete (next as {effects?:unknown}).effects;else Object.assign(next,{effects:value})}
      else if(key==='concatTitle'){if(!('style' in next))throw new Error('Concat title style requires a text layer');const style=object(value);if(!Object.keys(style).length||Object.keys(style).some(key=>!(NATIVE_CONCAT_TITLE_PATCH_FIELDS as readonly string[]).includes(key)))throw new Error('Unsupported Concat title style field');Object.assign(next,{concatTitle:{...next.concatTitle,...style}})}
      else if(key==='style'){if(!('style' in next))throw new Error('Text style requires a text layer');if(next.concatTitle)throw new Error('Retained Concat titles use concatTitle style fields');Object.assign(next,{style:patchTextStyle(project,scope,next.style,value)})}
      else if(key==='blendMode'){if((!('blendMode' in next)&&!('concatTitle' in next&&next.concatTitle))||typeof value!=='string'||!(COMPOSITION_BLEND_MODES as readonly string[]).includes(value)||('concatTitle' in next&&next.concatTitle&&!['normal','multiply','screen','addition','lighten','darken'].includes(value)))throw new Error('Invalid blend mode for layer');Object.assign(next,{blendMode:value})}
      else if(key==='sourceAudioEnabled'){if(track.kind!=='video'||typeof value!=='boolean')throw new Error('sourceAudioEnabled requires a video layer and boolean');const source=project.compositions[scope]!.content.sources[(next as {sourceId:string}).sourceId];if(value&&!source?.hasAudio)throw new Error('Video source does not contain audio');Object.assign(next,{sourceAudioEnabled:value})}
      else if(['gain','pan','audioGain','audioPan'].includes(key)){const embedded=key==='audioGain'||key==='audioPan',pan=key==='pan'||key==='audioPan';if(track.kind!==(embedded?'video':'audio'))throw new Error('Audio property does not match layer kind');if(typeof value!=='number'||!Number.isFinite(value)||value<(pan?-1:0)||value>(pan?1:MAX_AUDIO_GAIN))throw new Error('Invalid audio value');Object.assign(next,{[key]:{mode:'constant',value}})}
      else if(key==='enabled'){if(typeof value!=='boolean')throw new Error('enabled must be boolean');if(((next as {enabled?:boolean}).enabled!==false)!==value)Object.assign(next,{enabled:value})}
      else{if(typeof value!=='number'||!Number.isFinite(value))throw new Error('Expected finite layer value');if(key==='opacity'){if(!('opacity' in next)||value<0||value>1)throw new Error('Invalid opacity');Object.assign(next,{opacity:{mode:'constant',value}})}else{if(!('transform' in next))throw new Error('Layer has no visual transform');Object.assign(next,setNativeVisualConstant(next,key,value))}}
     }
     return next
    })} as WireCompositionTrack)));break
   }
   case 'update_canvas':{
    fields(op,['patch']);const patch=object(op.patch),wire=buildCompositionWireDocument(patchCompositionCanvas(seed(project,scope),patch)).canvas,node=project.compositions[scope]!,canvas={...node.content.canvas}
    for(const key of Object.keys(patch)){if(key==='fps'||key==='fpsRate'){canvas.fpsMilli=wire.fpsMilli;if(wire.fpsRate)canvas.fpsRate=wire.fpsRate;else delete canvas.fpsRate}else if(key==='backgroundColor')canvas.background=wire.background;else Object.assign(canvas,{[key]:wire[key as keyof typeof wire]})}
    project={...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,canvas}}}};break
   }
   case 'add_transition':case 'remove_transition':{
    fields(op,name==='add_transition'?['trackId','transition']:['trackId','transitionId']);const trackId=id(op.trackId),target=project.compositions[scope]!.content.tracks.find(track=>track.id===trackId)
    if(!target||target.kind!=='video'||target.locked)throw new Error('Video track missing or locked')
    const transition=name==='add_transition'?object(op.transition):null,transitionId=id(transition?.id??op.transitionId)
    if(!transition&&!target.transitions.some(entry=>entry.id===transitionId))throw new Error('Transition not found')
    project=tracks(project,scope,rows=>rows.map(track=>track.id===trackId&&track.kind==='video'?{...track,transitions:transition?[...track.transitions.filter(entry=>entry.id!==transitionId),transition as unknown as typeof track.transitions[number]]:track.transitions.filter(entry=>entry.id!==transitionId)}:track));break
   }
   case 'create_link':case 'remove_link':{
    fields(op,name==='create_link'?['id','memberIds','reassign']:['id']);const linkId=id(op.id),node=project.compositions[scope]!,groups=node.content.metadata?.clipLinks??{}
    if(name==='remove_link'){if(!Object.hasOwn(groups,linkId))throw new Error('Link not found');project=unlinkNativeClips(project,scope,groups[linkId]!);break}
    if(Object.hasOwn(groups,linkId))throw new Error('Link ID exists')
    const members=ids(op.memberIds);if(op.reassign!==undefined&&typeof op.reassign!=='boolean')throw new Error('reassign must be boolean');if(op.reassign===true)project=unlinkNativeClips(project,scope,members)
    const linked=linkNativeClips(project,scope,members),after=linked.compositions[scope]!,created=Object.keys(after.content.metadata!.clipLinks!).find(key=>!Object.hasOwn(project.compositions[scope]!.content.metadata?.clipLinks??{},key))!
    const next={...after.content.metadata!.clipLinks};delete next[created];next[linkId]=members
    project={...linked,compositions:{...linked.compositions,[scope]:{...after,content:{...after.content,metadata:{...after.content.metadata,clipLinks:next}}}}};break
   }
   default:if((NATIVE_PATH_OPERATIONS as readonly unknown[]).includes(name))project=await applyNativePathOperation(project,scope,op);else if((NATIVE_COLOR_OPERATIONS as readonly unknown[]).includes(name))project=applyNativeColorOperation(project,scope,op);else if((NATIVE_KEYFRAME_OPERATIONS as readonly unknown[]).includes(name))project=applyNativeKeyframeOperation(project,scope,op);else throw new Error(`Unsupported native batch operation: ${String(name)}`)
  }
 }
 return project
}
