import type {NativeNestedProject} from '../api'
import {createComposition} from '../composition/commands'
import {textClipToWire} from '../composition/payload'
import {isStableId} from '../composition/validation'
import {MAX_COMPOSITION_CLIPS,MAX_COMPOSITION_TRACKS,MAX_COMPOSITION_DURATION_TICKS,type WireCompositionTrack} from '../composition/types'
import {parseSubtitleDocument} from './import'
import {cuesToTextClips} from './srt'

/** Import copies into a single authored composition; existing instances keep their windows. */
export function importNestedSubtitles(project:NativeNestedProject, compositionId:string, serialized:string, allocate:(prefix:string)=>string, offsetUs=0) {
  const node=Object.hasOwn(project.compositions,compositionId)?project.compositions[compositionId]:undefined
  if(!node)throw new Error('Композиция не найдена')
  if(!Number.isSafeInteger(offsetUs)||offsetUs<0)throw new Error('Некорректное смещение субтитров')
  const cues=parseSubtitleDocument(serialized).map(cue=>({...cue,startTicks:cue.startTicks+offsetUs,endTicks:cue.endTicks+offsetUs})).sort((a,b)=>a.startTicks-b.startTicks)
  if(!cues.length)throw new Error('Файл не содержит субтитров')
  if(cues.some(cue=>!Number.isSafeInteger(cue.endTicks)||cue.endTicks>MAX_COMPOSITION_DURATION_TICKS))throw new Error('Субтитры выходят за пределы композиции')
  if(node.content.tracks.reduce((sum,track)=>sum+track.clips.length,0)+cues.length>MAX_COMPOSITION_CLIPS)throw new Error('Превышен лимит клипов композиции')
  const used=new Set(Object.values(project.compositions).flatMap(composition=>composition.content.tracks.flatMap(track=>[track.id,...track.clips.map(clip=>clip.id)])))
  const nextId=(prefix:string)=>{const id=allocate(prefix);if(!isStableId(id)||used.has(id))throw new Error('Нужен уникальный идентификатор субтитра');used.add(id);return id}
  const canvas=node.content.canvas
  const conversion=createComposition({width:canvas.width,height:canvas.height,fps:canvas.fpsMilli/1000,backgroundColor:'#000000FF'})
  const tracks=[...node.content.tracks]
  const roles=new Set(node.content.metadata?.captionTextTracks??[])
  const candidates=tracks.filter(track=>track.kind==='text'&&roles.has(track.id)&&!track.locked).map(track=>track.id)
  let firstTrackId='',firstClipId=''
  for(const cue of cues){
    let id=candidates.find(id=>{
      const track=tracks.find(track=>track.id===id)!
      return track.kind==='text'&&track.clips.every(clip=>cue.startTicks>=clip.timelineEndTick||clip.timelineStartTick>=cue.endTicks)
    })
    if(!id){
      if(tracks.length>=MAX_COMPOSITION_TRACKS)throw new Error('Превышен лимит дорожек композиции')
      id=nextId('subtitle-track')
      const last=tracks.reduce((previous,track,index)=>roles.has(track.id)?index:previous,-1)
      const names=new Set(tracks.map(track=>track.name));let serial=1
      while(names.has(`Субтитры ${serial}`))serial++
      tracks.splice(last+1,0,{kind:'text',id,name:`Субтитры ${serial}`,hidden:false,locked:false,clips:[]})
      roles.add(id);candidates.push(id)
    }
    const clip=textClipToWire(conversion,cuesToTextClips([cue],()=>nextId('subtitle'))[0]!)
    const index=tracks.findIndex(track=>track.id===id),track=tracks[index]!
    if(track.kind!=='text')throw new Error('Нужна текстовая дорожка')
    tracks[index]={...track,clips:[...track.clips,clip].sort((a,b)=>a.timelineStartTick-b.timelineStartTick)}
    if(!firstClipId){firstClipId=clip.id;firstTrackId=id}
  }
  const content={...node.content,tracks:tracks as readonly WireCompositionTrack[],metadata:{...node.content.metadata,captionTextTracks:tracks.filter(track=>track.kind==='text'&&roles.has(track.id)).map(track=>track.id)}}
  const end=Math.max(...cues.map(cue=>cue.endTicks))
  const document={...project,compositions:{...project.compositions,[compositionId]:{...node,...(node.durationUs!==undefined?{durationUs:Math.max(node.durationUs,end)}:{}),content}}}
  return {document,count:cues.length,firstTrackId,firstClipId}
}
