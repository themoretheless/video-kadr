import {expect,it} from 'vitest'
import {createComposition} from '../composition/commands'
import {buildCompositionWireDocument,textClipToWire} from '../composition/payload'
import {MAX_COMPOSITION_TRACKS} from '../composition/types'
import {importNestedSubtitles} from './nestedImport'
import {cuesToTextClips} from './srt'
import {parseSubtitleDocument} from './import'
import type {NativeNestedProject} from '../api'
const base=createComposition({width:320,height:180,fps:24,backgroundColor:'#000000FF'})
function project():NativeNestedProject{
 const content=buildCompositionWireDocument(base)
 return {schemaVersion:1,root:'root',compositions:{root:{durationUs:1_000_000,content,instances:[]},unused:{content,instances:[]}}}
}
const body='1\n00:00:01,125 --> 00:00:04,000\nLong\n\n2\n00:00:02,000 --> 00:00:03,000\nInner\n\n3\n00:00:03,000 --> 00:00:04,000\nAdjacent'
function allocator(){let serial=0;return(prefix:string)=>`${prefix}-${++serial}`}
it('packs overlapping cues, shares adjacent spans and uses canonical text wire transport',()=>{
 const input=project(),before=JSON.stringify(input)
 const result=importNestedSubtitles(input,'root',body,allocator())
 const node=result.document.compositions.root!,tracks=node.content.tracks
 expect(result.count).toBe(3)
 expect(tracks.map(track=>track.clips.length)).toEqual([1,2])
 expect(node.durationUs).toBe(4_000_000)
 const first=tracks[0]!.clips[0]!
 const expected=textClipToWire(base,cuesToTextClips(parseSubtitleDocument(body).slice(0,1),()=>first.id,base.canvas)[0]!)
 expect(first).toEqual(expected)
 expect(first).toHaveProperty('textBoxWidth', 269)
 expect(node.content.metadata?.captionTextTracks).toEqual(tracks.map(track=>track.id))
 expect(result.document.compositions.unused).toBe(input.compositions.unused)
 expect(JSON.stringify(input)).toBe(before)
 const reuse=importNestedSubtitles(result.document,'root','WEBVTT\n\n00:04.000 --> 00:05.000\nReuse',allocatorFor(result.document))
 expect(reuse.document.compositions.root!.content.tracks).toHaveLength(2)
 expect(reuse.firstTrackId).toBe(tracks[0]!.id)
})
function allocatorFor(input:NativeNestedProject){void input;let n=100;return(prefix:string)=>`${prefix}-${++n}`}
it('keeps ordinary titles separate and skips locked caption lanes without touching them',()=>{
 const first=importNestedSubtitles(project(),'root',body,allocator()).document
 const node=first.compositions.root!,caption=node.content.tracks[0]!
 const title={...caption,id:'title-track',name:'Titles',clips:[]}
 const input={...first,compositions:{...first.compositions,root:{...node,content:{...node.content,tracks:[title,...node.content.tracks.map(track=>({...track,locked:true}))]}}}}
 const before=JSON.stringify(input)
 const imported=importNestedSubtitles(input,'root','1\n00:00:01,000 --> 00:00:02,000\nNew',allocatorFor(input))
 expect(imported.document.compositions.root!.content.tracks).toHaveLength(4)
 expect(imported.document.compositions.root!.content.tracks[0]).toBe(title)
 expect(imported.document.compositions.root!.content.tracks[1]).toEqual(input.compositions.root!.content.tracks[1])
 expect(JSON.stringify(input)).toBe(before)
})
it('supports ASS text/time and a safe cursor offset, refusing invalid input/ids/track overflow atomically',()=>{
 const input=project(),before=JSON.stringify(input)
 const result=importNestedSubtitles(input,'root','[Events]\nFormat: Start, End, Text\nDialogue: 0:00:01.12,0:00:02.50,Привет',allocator(),2_000_000)
 expect(result.document.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({timelineStartTick:3_120_000,timelineEndTick:4_500_000,text:'Привет'})
 expect(()=>importNestedSubtitles(input,'root',body,()=> 'same')).toThrow('уникальный')
 expect(()=>importNestedSubtitles(input,'root',body,allocator(),-1)).toThrow()
 expect(()=>importNestedSubtitles(input,'root','bad',allocator())).toThrow()
 expect(()=>importNestedSubtitles(input,'missing',body,allocator())).toThrow()
 const full={...input,compositions:{...input.compositions,root:{...input.compositions.root!,content:{...input.compositions.root!.content,tracks:Array.from({length:MAX_COMPOSITION_TRACKS},(_,i)=>({kind:'text' as const,id:`title-${i}`,name:'Title',hidden:false,locked:false,clips:[]}))}}}}
 expect(()=>importNestedSubtitles(full,'root',body,allocator())).toThrow('лимит дорожек')
 expect(JSON.stringify(input)).toBe(before)
})

it('retains ASS cue styling in the authored nested project', () => {
 const result = importNestedSubtitles(project(), 'root', String.raw`[V4+ Styles]
Format: Name, Fontname, Fontsize, PrimaryColour, Outline, Alignment, Bold, Italic
Style: Note,Arial,60,&H00FFFFFF,2,3,-1,0
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:02.00,Note,{\1c&H0000FF&\i1}Caption
Dialogue: 0:00:02.00,0:00:03.00,Note,{\b0}Regular
`, allocator())
 expect(result.document.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({
  text: 'Caption',
  style: { fontFamily: 'Arial', fontSize: 60, align: 'right', strokeWidth: 2, bold: true, italic: true,
   color: { red: 1, green: 0, blue: 0, alpha: 1 } },
 })
 const track = result.document.compositions.root!.content.tracks[0]!
 expect(track.clips[1]).not.toHaveProperty('style.bold')
})

it('maps ASS absolute positions to editable canvas offsets', () => {
 const result = importNestedSubtitles(project(), 'root', String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:01.00,{\pos(65.5, 38)}Caption
Dialogue: 0:00:01.00,0:00:02.00,{\pos(9999999, 38)}Invalid position
`, allocator())
 const clips = result.document.compositions.root!.content.tracks[0]!.clips
 expect(clips[0]).toMatchObject({
  text: 'Caption',
  textAnchor: [0.5, 1],
  transform: { x: { mode: 'constant', value: -94.5 }, y: { mode: 'constant', value: -52 } },
 })
 expect(clips[1]).toMatchObject({
  text: 'Invalid position',
  textAnchor: [0.5, 1],
  transform: { x: { mode: 'constant', value: 0 }, y: { mode: 'constant', value: 75.6 } },
 })
})

it('uses the named or inline ASS anchor, with safe-area positioning when pos is absent', () => {
 const result = importNestedSubtitles(project(), 'root', String.raw`[V4+ Styles]
Format: Name, Alignment
Style: Top,7
[Events]
Format: Start, End, Style, Text
Dialogue: 0:00:00.00,0:00:01.00,Top,Top left
Dialogue: 0:00:01.00,0:00:02.00,Top,{\an3\pos(270,150)}Bottom right
`, allocator())
 const clips = result.document.compositions.root!.content.tracks[0]!.clips
 expect(clips[0]).toMatchObject({
  textAnchor: [0, 0],
  transform: { x: { value: -134.4 }, y: { value: -75.6 } },
 })
 expect(clips[1]).toMatchObject({
  textAnchor: [1, 1], style: { align: 'right' },
  transform: { x: { value: 110 }, y: { value: 60 } },
 })
})

it('preserves ASS fade timing as clip-local opacity after importing at the cursor', () => {
 const result = importNestedSubtitles(project(), 'root', String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:01.00,0:00:03.00,{\fad(250, 500)}Fade
Dialogue: 0:00:03.00,0:00:04.00,{\fad(600, 800)}Overlap
Dialogue: 0:00:04.00,0:00:05.00,{\fad(1200, 0)}Fade spans cue
`, allocator(), 2_000_000)
 const clips = result.document.compositions.root!.content.tracks[0]!.clips
 expect(result.simplifiedCueCount).toBe(0)
 expect(clips[0]).toMatchObject({
  timelineStartTick: 3_000_000,
  opacity: { mode: 'keyframes', track: {
   interpolation: 'linear', keyframes: [
    { tick: 0, value: 0 }, { tick: 250_000, value: 1 },
    { tick: 1_500_000, value: 1 }, { tick: 2_000_000, value: 0 },
   ],
  } },
 })
 expect(clips[1]).toMatchObject({ opacity: { track: { keyframes: [
  { tick: 0, value: 0 }, { tick: 599_999, value: 599_999 / 600_000 },
  { tick: 600_000, value: 0.5 }, { tick: 1_000_000, value: 0 },
 ] } } })
 expect(clips[2]).toMatchObject({ opacity: { track: { keyframes: [
  { tick: 0, value: 0 }, { tick: 1_000_000, value: 5 / 6 },
 ] } } })
})

it('imports exact ASS fade bounds, including a transition already in progress', () => {
 const result = importNestedSubtitles(project(), 'root', String.raw`[Events]
Format: Start, End, Text
Dialogue: 0:00:00.00,0:00:02.00,{\fade(255,127,255,-100,100,750,750)}Timed
Dialogue: 0:00:02.00,0:00:03.00,{\fade(255,0,255,500,100,600,700)}Invalid
`, allocator())
 const [timed, invalid] = result.document.compositions.root!.content.tracks[0]!.clips
 expect(timed).toMatchObject({ opacity: { mode: 'keyframes', track: { interpolation: 'linear', keyframes: [
  { tick: 0, value: 64 / 255 },
  { tick: 100_000, value: 128 / 255 },
  { tick: 749_999, value: 128 / 255 },
  { tick: 750_000, value: 0 },
  { tick: 2_000_000, value: 0 },
 ] } } })
 expect(invalid).toMatchObject({ opacity: { mode: 'constant', value: 1 }, text: 'Invalid' })
 expect(result.simplifiedCueCount).toBe(1)
})
