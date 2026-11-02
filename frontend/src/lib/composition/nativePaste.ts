import {moveNativeNestedClips,type NativeNestedProject,type NativeClipMove} from '../api'
import {captureNativeClipContent,finishNativeClipCopy} from './nativeClipCopy'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
import {orderedTimelineIds} from './timelineOrder'
import {isStableId} from './validation'
export async function pasteNativeLayers(project:NativeNestedProject,scope:string,subjects:readonly {clipId:string;id:string}[],options:{trackId?:string;startTicks?:number;linkIds?:readonly string[]}={}):Promise<NativeNestedProject>{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)throw new Error('Composition not found')
 if(!Array.isArray(subjects)||!subjects.length||subjects.length>512||subjects.some(subject=>!subject||Object.keys(subject).some(field=>!['clipId','id'].includes(field))||!isStableId(subject.clipId)||!isStableId(subject.id))||new Set(subjects.map(subject=>subject.clipId)).size!==subjects.length||new Set(subjects.map(subject=>subject.id)).size!==subjects.length)throw new Error('Paste requires distinct subjects and copy IDs')
 const rows=node.content.tracks.flatMap(track=>track.clips.map(clip=>({track,clip}))),selected=new Map(subjects.map(subject=>[subject.clipId,subject.id]))
 const locations=subjects.map(subject=>{const row=rows.find(row=>row.clip.id===subject.clipId);if(!row)throw new Error('Paste source layer not found');return row})
 const reserved=new Set([...Object.keys(project.compositions),...node.content.tracks.map(track=>track.id),...rows.map(row=>row.clip.id),...(node.content.captionTracks??[]).flatMap(track=>track.clips.map(clip=>clip.id)),...node.content.tracks.flatMap(track=>track.kind==='video'?track.transitions.map(transition=>transition.id):[]),...Object.keys(node.content.metadata?.clipLinks??{})])
 for(const subject of subjects){if(reserved.has(subject.id))throw new Error('Paste copy ID exists');reserved.add(subject.id)}
 const groups=Object.entries(node.content.metadata?.clipLinks??{}).map(([id,members])=>[id,members.filter(id=>selected.has(id))] as const).filter(([,members])=>members.length>=2)
 if(options.linkIds!==undefined&&(!Array.isArray(options.linkIds)||options.linkIds.length!==groups.length||new Set(options.linkIds).size!==options.linkIds.length))throw new Error('Paste link IDs must match copied link subsets')
 const linkIds=groups.map((_,index)=>options.linkIds?.[index]??`${subjects[0]!.id}-link-${index}`)
 for(const id of linkIds){if(!isStableId(id)||reserved.has(id))throw new Error('Paste link ID exists or is invalid');reserved.add(id)}
 const origin=Math.min(...locations.map(row=>nativeClipStart(row.clip))),end=Math.max(...locations.map(row=>nativeClipEnd(row.clip))),anchor=locations[0]!
 const proposed=options.startTicks??nativeClipStart(anchor.clip)+end-origin
 if(!Number.isSafeInteger(proposed)||proposed<0)throw new Error('Invalid paste start')
 const delta=Math.max(-origin,proposed-nativeClipStart(anchor.clip))
 // Copy only the selected subsets; retain the original groups in the destination.
 const source={...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,metadata:{...node.content.metadata,clipLinks:Object.fromEntries(groups)}}}}}
 const queue=rows.filter(row=>selected.has(row.clip.id)).map(row=>selected.get(row.clip.id)!)
 let serial=0
 const allocate=()=>{const requested=queue.shift();if(requested)return requested;let id:string;do{id=`${subjects[0]!.id.slice(0,96)}-paste-${serial++}`}while(reserved.has(id));if(!isStableId(id))throw new Error('Invalid generated paste ID');reserved.add(id);return id}
 const captured=captureNativeClipContent(source,scope,subjects.map(subject=>subject.clipId),allocate)
 const copiedGroups=Object.values(captured.node.content.metadata?.clipLinks??{})
 if(copiedGroups.length!==linkIds.length)throw new Error('Copied link groups do not match the requested subset')
 const temporary=allocate()
 const scratch={...captured.node,content:{...captured.node.content,metadata:{...captured.node.content.metadata,clipLinks:Object.fromEntries(copiedGroups.map((members,index)=>[linkIds[index]!,members]))}}}
 const staged:NativeNestedProject={...project,...(project.timelineOrder?{timelineOrder:[...orderedTimelineIds(project),temporary]}:{}),compositions:{...project.compositions,[temporary]:scratch}}
 const moves:NativeClipMove[]=locations.map((row,index)=>{
  const targetId=index===0?options.trackId??row.track.id:row.track.id,target=node.content.tracks.find(track=>track.id===targetId)
  if(!target||target.kind!==row.track.kind||target.locked)throw new Error('Paste target is missing, locked or has a different kind')
  const start=nativeClipStart(row.clip)+delta
  if(!Number.isSafeInteger(start)||start<0)throw new Error('Invalid paste position')
  return {fromComposition:temporary,fromTrackId:row.track.id,clipId:subjects[index]!.id,toComposition:scope,toTrackId:targetId,startUs:start,createTrack:false}
 })
 return finishNativeClipCopy(await moveNativeNestedClips(staged,moves),temporary,project,scope)
}
