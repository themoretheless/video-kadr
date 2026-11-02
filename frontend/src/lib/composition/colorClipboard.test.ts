import { expect,it } from 'vitest'
import { createComposition,addTrack,addClip } from './commands'
import { copyColorKeys,pasteColorKeys } from './colorClipboard'
import type { TextClip } from './types'
it('copies detached RGBA and temporal curves and pastes at the target clock', () => {
  let source = createComposition({ width: 128,height: 128,fps: 30,backgroundColor: '#000000' })
  source = addTrack(source,{ id: 't',kind: 'text',name: 'Text',locked: false,hidden: false,clips: [] })
  source = addClip(source,'t',{ id: 'c',kind: 'text',timelineStartTicks: 1000000,durationTicks: 2000000,text: 'Color',x: 0,y: 0,opacity: 1,style: { fontSizePx: 24,color: '#FFFFFFFF',align: 'center' },colorAnimation: { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [.2,-.3],p2: [.8,1.4] },keyframes: [{ tick: 500,value: '#FF000080' },{ tick: 1000,value: '#0000FFFF' }] } } })
  const before = JSON.stringify(source)
  const groups = copyColorKeys(source,[{ clipId: 'c',tick: 500 },{ clipId: 'c',tick: 1000 }])
  expect(groups[0]!.keys.map(key => key.tick)).toEqual([0,500000])
  const sourceClip = source.tracks[0]!.clips[0] as TextClip
  if (sourceClip.colorAnimation?.mode !== 'keyframes') throw new Error('Expected keys')
  expect(groups[0]!.keys[0]!.interpolation).not.toBe(sourceClip.colorAnimation.track.interpolation)
  const pasted = pasteColorKeys(source,groups,['c'],1750000)
  expect(pasted.subjects).toEqual([{ clipId: 'c',tick: 750 },{ clipId: 'c',tick: 1250 }])
  const clip = pasted.document.tracks[0]!.clips[0] as TextClip
  expect(clip.colorAnimation).toMatchObject({ track: { keyframes: [{ tick: 500 },{ tick: 750,value: '#FF000080',interpolation: { kind: 'bezier',p1: [.2,-.3],p2: [.8,1.4] } },{ tick: 1000 },{ tick: 1250,value: '#0000FFFF' }] } })
  expect(() => pasteColorKeys(source,groups,['missing'],1750000)).toThrow()
  expect(() => pasteColorKeys(source,[...groups,...groups],['c','missing'],1750000)).toThrow()
  expect(() => copyColorKeys(source,[{ clipId: 'c',tick: 500 },{ clipId: 'c',tick: 500 }])).toThrow('Duplicate')
  expect(JSON.stringify(source)).toBe(before)
})
