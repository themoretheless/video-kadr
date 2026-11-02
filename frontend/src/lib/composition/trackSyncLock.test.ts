import {expect, test} from 'vitest'
import {addTrack,createComposition} from './commands'
import {normalizeComposition,validateComposition} from './validation'
import {buildCompositionWireDocument} from './payload'
import type {CompositionTrack} from './types'

test('all ordinary track kinds retain sync-lock at authoring persistence boundaries',()=>{
 let doc=createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'})
 for(const kind of ['video','audio','image','text','color','graphic'] as const){
  doc=addTrack(doc,{id:kind,name:kind,kind,clips:[],locked:false,hidden:false,muted:false,syncLock:false} as CompositionTrack)
 }
 const restored=normalizeComposition(JSON.parse(JSON.stringify(doc)))
 expect(restored.tracks.map(track=>track.syncLock)).toEqual([false,false,false,false,false,false])
 // The render contract excludes editing-only flags, like clip links.
 expect(buildCompositionWireDocument(restored).tracks.every(track=>!('syncLock' in track))).toBe(true)
 for(const value of [null,'false',0,{},[]]){
  const invalid={...doc,tracks:[{...doc.tracks[0],syncLock:value}]}
  expect(validateComposition(invalid).some(issue=>issue.code==='track-sync-lock')).toBe(true)
  expect(()=>normalizeComposition(invalid)).toThrow()
 }
 const legacy={...doc,tracks:doc.tracks.map(({syncLock,...track})=>track)}
 expect(validateComposition(legacy)).toEqual([])
 expect(normalizeComposition(legacy).tracks.every(track=>(track.syncLock??true))).toBe(true)
})
