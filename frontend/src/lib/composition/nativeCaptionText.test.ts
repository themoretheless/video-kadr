import {beforeEach,expect,it} from 'vitest'
import {captionAppearance, setNativeCaptionText} from './nativeCaptionText'
import type {WireTransform} from './types'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {importSrtToComposition} from '../state/composition/timeline.svelte'
import {buildCompositionWireDocument} from './payload'
import type {NativeNestedProject} from '../api'
let project:NativeNestedProject
beforeEach(()=>{
 setCompositionStorageForTests(null);resetCompositionForTests()
 importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOriginal')
 const content=buildCompositionWireDocument(compositionState.document)
 project={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},unused:{content,instances:[]}}}
})
it('changes only literal caption text, preserving clocks/style/animation/other compositions and input',()=>{
 const node=project.compositions.root!,track=node.content.tracks[0]!,clip=track.clips[0]!,before=JSON.stringify(project)
 const changed=setNativeCaptionText(project,'root',clip.id,'Исправлено <&>\nNext line','Original')
 expect(changed.compositions.root!.content.tracks[0]!.clips[0]).toEqual({...clip,text:'Исправлено <&>\nNext line'})
 expect(changed.compositions.unused).toBe(project.compositions.unused)
 expect(JSON.stringify(project)).toBe(before)
 expect(setNativeCaptionText(project,'root',clip.id,'Original','Original')).toBe(project)
})
it('refuses stale text, locks, normal titles and invalid content before any mutation',()=>{
 const node=project.compositions.root!,track=node.content.tracks[0]!,clip=track.clips[0]!,before=JSON.stringify(project)
 for(const text of ['','  ','Bad\0text','A'.repeat(513)])expect(()=>setNativeCaptionText(project,'root',clip.id,text,'Original')).toThrow()
 expect(()=>setNativeCaptionText(project,'root',clip.id,'New','Stale')).toThrow('изменился')
 const title={...project,compositions:{...project.compositions,root:{...node,content:{...node.content,metadata:{}}}}}
 expect(()=>setNativeCaptionText(title,'root',clip.id,'New','Original')).toThrow('не является')
 const locked={...project,compositions:{...project.compositions,root:{...node,content:{...node.content,tracks:[{...track,locked:true}]}}}}
 expect(()=>setNativeCaptionText(locked,'root',clip.id,'New','Original')).toThrow('заблокирована')
 expect(()=>setNativeCaptionText(project,'missing',clip.id,'New','Original')).toThrow()
 expect(JSON.stringify(project)).toBe(before)
})
it('counts Unicode code points without truncating surrogate pairs',()=>{
 const id=project.compositions.root!.content.tracks[0]!.clips[0]!.id
 expect(setNativeCaptionText(project,'root',id,'🙂'.repeat(512),'Original').compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({text:'🙂'.repeat(512)})
 expect(()=>setNativeCaptionText(project,'root',id,'🙂'.repeat(513),'Original')).toThrow()
})

it('shifts an entire Bezier path and keeps its relative handles and clock', () => {
  const root = project.compositions.root!
  const track = root.content.tracks[0]!
  if (track.kind !== 'text') throw new Error('expected text track')
  const clip = track.clips[0]!
  const path: NonNullable<WireTransform['position']> = {
    mode: 'path', progress: { mode: 'constant', value: 0.5 },
    path: { nodes: [{ id: 'start', point: { x: 10, y: 20 }, inHandle: { x: 0, y: 0 },
      outHandle: { x: 2, y: 3 }, segment: 'cubic', tangentMode: 'corner' },
    { id: 'end', point: { x: 30, y: 40 }, inHandle: { x: -2, y: -3 },
      outHandle: { x: 0, y: 0 }, segment: 'line', tangentMode: 'corner' }] },
  }
  const entry = { ...clip, transform: { ...clip.transform, position: path } }
  const document = { ...project, compositions: { ...project.compositions, root: { ...root,
    content: { ...root.content, tracks: [{ ...track, clips: [entry] }] },
  } } }
  const expected = captionAppearance(entry)
  const changed = setNativeCaptionText(document, 'root', clip.id, clip.text, clip.text, {
    expected, value: { ...expected, x: 16, y: 17 },
  })
  expect(changed.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({
    transform: { position: { ...path, path: { nodes: path.path.nodes.map(node => ({
      ...node, point: { x: node.point.x + 6, y: node.point.y - 3 },
    })) } } },
  })
})

it('shifts and resizes Concat motion while preserving its keys and easing', () => {
  const root = project.compositions.root!
  const track = root.content.tracks[0]!
  if (track.kind !== 'text') throw new Error('expected text track')
  const clip = track.clips[0]!
  const concatAnimation = {
    canvas: [320, 180], durationTicks: 1000000, stretch: [1, 1],
    rest: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotationDegrees: 0, opacity: 1 },
    keys: { offsetX: [{ at: 0.25, value: 0.5, ease: [0.2, 0.1, 0.8, 0.9] }],
      scale: [{ at: 0.5, value: 1.3, ease: 'out' }], rotation: [{ at: 0.8, value: -15, ease: 'in' }],
      opacity: [{ at: 0, value: 0.4 }] },
  } as const
  const entry = { ...clip, transform: { ...clip.transform, concatAnimation } }
  const document = { ...project, compositions: { ...project.compositions, root: { ...root,
    content: { ...root.content, tracks: [{ ...track, clips: [entry] }] },
  } } }
  const expected = captionAppearance(entry)
  const changed = setNativeCaptionText(document, 'root', clip.id, clip.text, clip.text, {
    expected, value: { ...expected, x: 40, y: 0, scaleX: 2, scaleY: 0.5, rotation: 20 },
  })
  expect(changed.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({
    transform: { concatAnimation: { ...concatAnimation, stretch: [2, 0.5],
      rest: { ...concatAnimation.rest, x: 0.125, rotationDegrees: 20 }, keys: {
      ...concatAnimation.keys, offsetX: [{ ...concatAnimation.keys.offsetX[0], value: 0.625 }],
      rotation: [{ ...concatAnimation.keys.rotation[0], value: 5 }],
    } } },
  })
})
