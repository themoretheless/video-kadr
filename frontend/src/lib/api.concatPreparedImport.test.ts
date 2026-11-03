import {afterEach,expect,it,vi} from 'vitest'
import {importConcatProjectWithSavedStills,importConcatProjectWithSavedCutouts} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('imports via authenticated saved references with distinct source and destination identities',async()=>{
 const report={schemaVersion:1,status:'needs_attention',project:null,diagnostics:['unsupported field'],sourceDocument:{version:1},timelineIds:{}}
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(report)));vi.stubGlobal('fetch',fetchMock)
 const source={id:'physical',kind:'image' as const,durationTicks:0,width:16,height:16,hasAudio:false}
 const references=[{timelineId:'destination',clipId:'destination-clip',projectId:'private',compositionId:'source-timeline',sourceClipId:'source-clip',expectedRevision:7}]
 const cancel=new AbortController()
 expect(await importConcatProjectWithSavedStills({version:1},{slot:source},references,cancel.signal,'owner',{'font.ttf':{fallback:true}},true)).toEqual(report)
 const [url,options]=fetchMock.mock.calls[0]!;expect(url).toBe('/api/compositions/concat-project/import-ready');expect(options.signal).toBe(cancel.signal);expect(new Headers(options.headers).get('authorization')).toBe('Bearer owner')
 expect(JSON.parse(options.body)).toEqual({request:{schemaVersion:1,document:{version:1},mediaBindings:{slot:source},preserveMediaIds:true,fontBindings:{'font.ttf':{fallback:true}}},preparedStills:references})
})
it('propagates stale or unauthorized saved references without falling back to unverified import',async()=>{
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify({error:'stale saved cutout revision'}),{status:409}));vi.stubGlobal('fetch',fetchMock)
 await expect(importConcatProjectWithSavedStills({}, {},[],undefined,'owner')).rejects.toThrow('stale saved cutout revision');expect(fetchMock).toHaveBeenCalledTimes(1)
})

it('sends prepared video references without accepting client mask metadata',async()=>{
 const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify({status:'converted'})));vi.stubGlobal('fetch',fetchMock)
 const references=[{timelineId:'main',clipId:'destination',projectId:'saved',compositionId:'root',sourceClipId:'source',expectedRevision:4}]
 await importConcatProjectWithSavedCutouts({}, {},[],references,undefined,'owner',{},true)
 expect(JSON.parse(fetchMock.mock.calls[0]![1].body)).toEqual({request:{schemaVersion:1,document:{},mediaBindings:{},preserveMediaIds:true},preparedStills:[],preparedVideos:references})
})
