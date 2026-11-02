import {afterEach,expect,it,vi} from 'vitest'
import {readCaptionFile,writeCaptionFile,type CaptionFileDocument} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('retains signed source ticks beyond JSON numeric precision and WebVTT metadata',async()=>{
 const document:CaptionFileDocument={ticksPerSecond:'254016000000',cues:[{startTick:'-9007199254740993',endTick:'9007199254740993',text:'Привет',speaker:'Voice',id:'cue-1',settings:'align:start'}],blocks:['NOTE original'],warnings:['retained warning']}
 const fetchMock=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({format:'vtt',document}))).mockResolvedValueOnce(new Response(JSON.stringify({extension:'vtt',dataBase64:'V0VCVlRU'})))
 vi.stubGlobal('fetch',fetchMock)
 const signal=new AbortController().signal
 const parsed=await readCaptionFile('V0VCVlRU',{extension:'vtt'},signal)
 expect(parsed.document).toEqual(document)
 await writeCaptionFile(parsed.document,'vtt',{dropFrame:false,rate:{num:24000,den:1001}},signal)
 const init=fetchMock.mock.calls[1][1] as RequestInit
 const body=JSON.parse(init.body as string)
 expect(body.document.cues[0].startTick).toBe('-9007199254740993')
 expect(body.document.cues[0].endTick).toBe('9007199254740993')
 expect(body.document.blocks).toEqual(['NOTE original'])
 expect(body.rate).toEqual({num:24000,den:1001})
 expect(init.signal).toBe(signal)
})

it('transports import frame rate and exact export range offsets without numeric conversion',async()=>{
 const {prepareCaptionFileImport}=await import('./api')
 const document:CaptionFileDocument={ticksPerSecond:'254016000000',cues:[{startTick:'137729592000',endTick:'508540032000',text:'Hello',id:'cue-1',speaker:'Ann',settings:'align:start'}],blocks:['NOTE retained']}
 const prepared={name:'talk',format:'Subtitle',language:'en',enabled:true,locked:false,syncLock:true,backgroundColor:[0,0,0,191],document}
 const fetchMock=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify(prepared))).mockResolvedValueOnce(new Response(JSON.stringify({extension:'vtt',dataBase64:'V0VCVlRU'})))
 vi.stubGlobal('fetch',fetchMock)
 await expect(prepareCaptionFileImport('V0VCVlRU','/captions/talk.vtt',{num:24000,den:1001})).resolves.toEqual(prepared)
 expect(fetchMock.mock.calls[0][0]).toBe('/api/compositions/captions/file/prepare-import')
 expect(JSON.parse(fetchMock.mock.calls[0][1].body)).toMatchObject({rate:{num:24000,den:1001},path:'/captions/talk.vtt'})
 await writeCaptionFile(document,'vtt',{offsetTick:'9007199254740993'})
 expect(JSON.parse(fetchMock.mock.calls[1][1].body).offsetTick).toBe('9007199254740993')
})
