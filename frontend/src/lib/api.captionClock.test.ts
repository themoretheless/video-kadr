import {afterEach,expect,it,vi} from 'vitest'
import {editCompositionCaptions,type CaptionEditOperation} from './api'
import {createComposition} from './composition/commands'
import {buildCompositionWireDocument} from './composition/payload'
afterEach(()=>vi.unstubAllGlobals())
it('transports exact caption command positions and signed deltas without numeric conversion',async()=>{
 const composition=buildCompositionWireDocument(createComposition({width:320,height:180,fps:24,backgroundColor:'#000000'}))
 const operations:CaptionEditOperation[]=[
  {kind:'add',trackId:'captions',startTick:'9007199254740993',endTick:'9007453270740993'},
  {kind:'split',id:'cue',timeTick:'137729592001'},
  {kind:'times',id:'cue',startTick:'1',endTick:'254016000001'},
  {kind:'trim',id:'cue',inEdge:true,deltaTicks:'-1'},
  {kind:'move',ids:['cue'],deltaTicks:'1'},
  {kind:'add',trackId:'captions',startInput:{unit:'frames',value:13},endInput:{unit:'frames',value:16}},
  {kind:'times',id:'cue',endInput:{unit:'timecode',value:'+15'}},
 ]
 const fetchMock=vi.fn().mockImplementation(()=>Promise.resolve(new Response(JSON.stringify({composition,appliedDelta:'1',deleted:0}))))
 vi.stubGlobal('fetch',fetchMock)
 for(const operation of operations){
  const result=await editCompositionCaptions(composition,operation)
  const [url,init]=fetchMock.mock.lastCall!
  expect(url).toBe('/api/compositions/captions/edit')
  expect(JSON.parse((init as RequestInit).body as string).operation).toEqual(operation)
  expect(result.appliedDelta).toBe('1')
 }
})

it('transports explicit relative-timecode context and drop-frame preference',async()=>{
 const composition=buildCompositionWireDocument(createComposition({width:320,height:180,fps:30000/1001,fpsRate:{numerator:30000,denominator:1001},backgroundColor:'#000000'}))
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify({composition,appliedDelta:null,deleted:0})))
 vi.stubGlobal('fetch',fetchMock)
 const operation:CaptionEditOperation={kind:'times',id:'cue',endInput:{unit:'timecode',value:'+15'}}
 const context={playheadTick:'847566720000',dropFrame:true}
 await editCompositionCaptions(composition,operation,undefined,context)
 expect(JSON.parse((fetchMock.mock.calls[0]![1] as RequestInit).body as string)).toEqual({composition,operation,...context})
})
