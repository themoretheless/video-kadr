import { expect, it } from 'vitest'
import type { NativeNestedProject } from '$lib/api'
import { createComposition } from './commands'
import { buildCompositionWireDocument } from './payload'
import { editNativeTrack, type NativeTrackKind } from './nativeTrackEdit'
function project(): NativeNestedProject { const content = buildCompositionWireDocument(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' })); return { schemaVersion: 1, root: 'root', compositions: { root: { content, instances: [] } } } }
it('creates every native lane kind, supports only its flags, preserves input and no-op identity', () => {
  for (const trackKind of ['video', 'audio', 'image', 'text', 'color', 'graphic'] as NativeTrackKind[]) {
    const input = project(), before = JSON.stringify(input)
    const added = editNativeTrack(input, 'root', { kind: 'add', id: 'new', trackKind, name: ' Lane ' })
    const track = added.compositions.root!.content.tracks.at(-1)!
    expect(track).toMatchObject({ id: 'new', kind: trackKind, name: 'Lane', clips: [], locked: false })
    const locked = editNativeTrack(added, 'root', { kind: 'flag', id: 'new', flag: 'locked', value: true })
    expect(editNativeTrack(locked, 'root', { kind: 'flag', id: 'new', flag: 'locked', value: true })).toBe(locked)
    expect(() => editNativeTrack(locked, 'root', { kind: 'remove', id: 'new', removeClips: true })).toThrow('заблокирована')
    const unlocked = editNativeTrack(locked, 'root', { kind: 'flag', id: 'new', flag: 'locked', value: false })
    const renamed = editNativeTrack(unlocked, 'root', { kind: 'rename', id: 'new', name: 'Renamed' })
    expect(editNativeTrack(renamed, 'root', { kind: 'rename', id: 'new', name: ' Renamed ' })).toBe(renamed)
    expect(() => editNativeTrack(added, 'root', { kind: 'flag', id: 'new', flag: trackKind === 'audio' ? 'hidden' : 'solo', value: true })).toThrow('недоступен')
    expect(editNativeTrack(added, 'root', { kind: 'remove', id: 'new', removeClips: false }).compositions.root!.content.tracks).toEqual(input.compositions.root!.content.tracks)
    expect(JSON.stringify(input)).toBe(before)
  }
})
it('rejects unknown timelines/tracks, duplicate IDs, blank names and track overflow', () => {
  const input = project(), add = { kind: 'add', id: 'new', trackKind: 'video', name: 'V' } as const
  expect(() => editNativeTrack(input, 'missing', add)).toThrow()
  expect(() => editNativeTrack(input, 'root', { kind: 'remove', id: 'missing', removeClips: false })).toThrow()
  const added = editNativeTrack(input, 'root', add)
  expect(() => editNativeTrack(added, 'root', add)).toThrow()
  expect(() => editNativeTrack(input, 'root', { ...add, name: ' ' })).toThrow()
  let full = input
  while (full.compositions.root!.content.tracks.length < 16) full = editNativeTrack(full, 'root', { ...add, id: `lane-${full.compositions.root!.content.tracks.length}` })
  expect(() => editNativeTrack(full, 'root', add)).toThrow('предел')
})
it('requires explicit content removal and prunes only deleted clip metadata while retaining sources', async () => {
  const { addTrack, addClip, registerSource } = await import('./commands')
  let editor = registerSource(createComposition({ width: 16, height: 16, fps: 10, backgroundColor: '#000000' }), { id: 'source', kind: 'video', durationTicks: 2000000, width: 16, height: 16, hasAudio: false })
  editor = addTrack(editor, { id: 'v', kind: 'video', name: 'V', hidden: false, muted: false, locked: false, clips: [] })
  editor = addClip(editor, 'v', { id: 'picture', kind: 'video', sourceId: 'source', timelineStartTicks: 0, sourceInTicks: 0, sourceOutTicks: 1000000, transform: { x: 0, y: 0, width: 16, height: 16, fit: 'contain' }, opacity: 1, sourceAudioEnabled: false, audioGain: 1 })
  const content = buildCompositionWireDocument(editor)
  const input: NativeNestedProject = { schemaVersion: 1, root: 'root', compositions: { root: { content: { ...content, metadata: { ...content.metadata, clipNames: { picture: 'Picture' }, mediaVersions: { picture: { originalSourceId: 'source', copySourceId: 'source' } } } }, instances: [] } } }
  const before = JSON.stringify(input)
  expect(() => editNativeTrack(input, 'root', { kind: 'remove', id: 'v', removeClips: false })).toThrow('Подтвердите')
  const output = editNativeTrack(input, 'root', { kind: 'remove', id: 'v', removeClips: true })
  expect(output.compositions.root!.content.tracks.some(track => track.id === 'v')).toBe(false)
  expect(output.compositions.root!.content.metadata?.clipNames).toEqual({})
  expect(output.compositions.root!.content.metadata?.mediaVersions).toEqual({})
  expect(output.compositions.root!.content.sources).toBe(input.compositions.root!.content.sources)
  expect(JSON.stringify(input)).toBe(before)
})

function groupedProject(): NativeNestedProject {
  const input=project(),node=input.compositions.root!,base={name:'Source lane',locked:false,hidden:false,clips:[]}
  return {...input,compositions:{root:{...node,content:{...node.content,tracks:[{...base,id:'image',kind:'image'},{...base,id:'video',kind:'video',muted:false,transitions:[]},{...base,id:'other',kind:'image'}],metadata:{concatLaneGroups:{video:['video','image']}}}}}}
}
it('edits the explicitly grouped source lane from either sibling without changing other lanes',()=>{
  for(const id of ['video','image']){
    const input=groupedProject(),before=JSON.stringify(input)
    const hidden=editNativeTrack(input,'root',{kind:'flag',id,flag:'hidden',value:true})
    expect(hidden.compositions.root!.content.tracks.slice(0,2).map(track=>'hidden' in track&&track.hidden)).toEqual([true,true])
    expect(hidden.compositions.root!.content.tracks[2]).toBe(input.compositions.root!.content.tracks[2])
    expect(editNativeTrack(hidden,'root',{kind:'flag',id,flag:'hidden',value:true})).toBe(hidden)
    const locked=editNativeTrack(hidden,'root',{kind:'flag',id,flag:'locked',value:true})
    expect(locked.compositions.root!.content.tracks.slice(0,2).every(track=>track.locked)).toBe(true)
    expect(()=>editNativeTrack(locked,'root',{kind:'rename',id,name:'New'})).toThrow('заблокирована')
    const unlocked=editNativeTrack(locked,'root',{kind:'flag',id,flag:'locked',value:false})
    const renamed=editNativeTrack(unlocked,'root',{kind:'rename',id,name:'New'})
    expect(renamed.compositions.root!.content.tracks.slice(0,2).map(track=>track.name)).toEqual(['New','New'])
    const removed=editNativeTrack(renamed,'root',{kind:'remove',id,removeClips:false})
    expect(removed.compositions.root!.content.tracks.map(track=>track.id)).toEqual(['other'])
    expect(removed.compositions.root!.content.metadata?.concatLaneGroups).toEqual({})
    expect(JSON.stringify(input)).toBe(before)
  }
})
it('does not infer sibling membership and refuses stale or overlapping group identities',()=>{
  const input=groupedProject(),content=input.compositions.root!.content
  const independent={...input,compositions:{root:{...input.compositions.root!,content:{...content,metadata:{}}}}}
  const hidden=editNativeTrack(independent,'root',{kind:'flag',id:'video',flag:'hidden',value:true})
  expect(hidden.compositions.root!.content.tracks[0]).toBe(content.tracks[0])
  const broken={...input,compositions:{root:{...input.compositions.root!,content:{...content,tracks:[content.tracks[1]!,content.tracks[0]!,content.tracks[2]!]}}}}
  expect(()=>editNativeTrack(broken,'root',{kind:'flag',id:'video',flag:'hidden',value:true})).toThrow('порядок')
  const duplicate={...input,compositions:{root:{...input.compositions.root!,content:{...content,metadata:{concatLaneGroups:{video:['video','image'],other:['other','image']}}}}}}
  expect(()=>editNativeTrack(duplicate,'root',{kind:'remove',id:'image',removeClips:true})).toThrow()
})
it('requires confirmation for content in either sibling and removes both clips and only their metadata',async()=>{
  const {addTrack,addClip,registerSource}=await import('./commands')
  let editor=registerSource(createComposition({width:16,height:16,fps:25,backgroundColor:'#000000'}),{id:'source',kind:'video',durationTicks:2000000,width:16,height:16,hasAudio:false})
  editor=addTrack(editor,{id:'v',kind:'video',name:'V',hidden:false,muted:false,locked:false,clips:[]})
  editor=addClip(editor,'v',{id:'picture',kind:'video',sourceId:'source',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1})
  const wire=buildCompositionWireDocument(editor),video=wire.tracks.find(track=>track.kind==='video')!
  if(video.kind!=='video')throw new Error('fixture video missing')
  const picture=video.clips[0]!,base=groupedProject(),node=base.compositions.root!,content=node.content
  const input:NativeNestedProject={...base,compositions:{root:{...node,content:{...content,sources:{...wire.sources,still:{id:'still',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}},tracks:content.tracks.map(track=>track.kind==='image'&&track.id==='image'?{...track,clips:[{id:'hold',sourceId:'still',timelineStartTick:1000000,durationTicks:1000000,transform:picture.transform,opacity:picture.opacity,blendMode:picture.blendMode,effects:[],enabled:true}]}:track),metadata:{...content.metadata,clipNames:{hold:'Hold',other:'Keep'}}}}}}
  expect(()=>editNativeTrack(input,'root',{kind:'remove',id:'video',removeClips:false})).toThrow('Подтвердите')
  const removed=editNativeTrack(input,'root',{kind:'remove',id:'video',removeClips:true})
  expect(removed.compositions.root!.content.tracks.map(track=>track.id)).toEqual(['other'])
  expect(removed.compositions.root!.content.metadata?.clipNames).toEqual({other:'Keep'})
  expect(removed.compositions.root!.content.sources).toBe(input.compositions.root!.content.sources)
  expect(input.compositions.root!.content.tracks[0]!.clips).toHaveLength(1)
})
