import {afterEach,expect,it,vi} from 'vitest'
import {getProjectCutoutSettings,getProjectCutoutOriginal,startProjectCutoutCorrection,getCutoutOriginal,getCutoutCorrection,startCutoutCorrection,getCutoutStatus,startCutout,getCutoutImage,saveCutoutImage,listCutouts,listCutoutModels,selectCutoutModel,removeCutoutModel,startCutoutModelDownload,listCutoutModelDownloads} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('uses authenticated abortable source-ID cutout routes without exposing paths or model settings',async()=>{
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{status:200,headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const signal=new AbortController().signal
 await getCutoutStatus('owner',signal);await startCutout('owner','source /?',signal);await saveCutoutImage('owner','job /?','project',signal);await listCutouts('owner','source /?','old /?',signal)
 expect(fetch.mock.calls.map(call=>String(call[0]))).toEqual(['/api/cutout/status','/api/cutout/jobs','/api/cutout/jobs/job%20%2F%3F/asset','/api/cutout/jobs?sourceId=source+%2F%3F&cursor=old+%2F%3F'])
 for(const call of fetch.mock.calls){expect(new Headers(call[1]?.headers).get('authorization')).toBe('Bearer owner');expect(call[1]?.signal).toBe(signal)}
 expect(JSON.parse(String(fetch.mock.calls[1]![1]?.body))).toEqual({sourceId:'source /?'})
 expect(JSON.parse(String(fetch.mock.calls[2]![1]?.body))).toEqual({projectId:'project'})
})
it('rejects a failed, non-PNG or undersized private preview and returns a valid PNG blob',async()=>{
 const signal=new AbortController().signal
 const fetch=vi.fn().mockResolvedValueOnce(new Response('forbidden',{status:403})).mockResolvedValueOnce(new Response('wrong',{headers:{'content-type':'text/plain'}})).mockResolvedValueOnce(new Response('tiny',{headers:{'content-type':'image/png'}})).mockResolvedValueOnce(new Response(new Uint8Array(64),{headers:{'content-type':'image/png'}}));vi.stubGlobal('fetch',fetch)
 await expect(getCutoutImage('owner','id',signal)).rejects.toThrow();await expect(getCutoutImage('owner','id',signal)).rejects.toThrow('PNG');await expect(getCutoutImage('owner','id',signal)).rejects.toThrow('размер');expect((await getCutoutImage('owner','id',signal)).size).toBe(64)
})

it('keeps model catalog, selection, removal and download history owner authenticated',async()=>{
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{status:200,headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const signal=new AbortController().signal
 await listCutoutModels('owner',signal);await selectCutoutModel('owner','isnet-general-use',signal);await removeCutoutModel('owner','isnet-general-use',signal);await startCutoutModelDownload('owner','isnet-general-use',signal);await listCutoutModelDownloads('owner','old /?',signal)
 expect(fetch.mock.calls.map(call=>String(call[0]))).toEqual(['/api/cutout/models','/api/cutout/models/select','/api/cutout/models/remove','/api/cutout/models/downloads','/api/cutout/models/downloads?cursor=old%20%2F%3F'])
 for(const call of fetch.mock.calls){expect(new Headers(call[1]?.headers).get('authorization')).toBe('Bearer owner');expect(call[1]?.signal).toBe(signal)}
 expect(fetch.mock.calls.slice(1,4).map(call=>JSON.parse(String(call[1]?.body)))).toEqual([{modelId:'isnet-general-use'},{modelId:'isnet-general-use'},{catalogId:'isnet-general-use'}])
})

it('keeps original and full mask recipes authenticated, abortable and independent from model installation',async()=>{
 const fetch=vi.fn().mockResolvedValueOnce(new Response(new Uint8Array(64),{headers:{'content-type':'image/png'}})).mockResolvedValueOnce(new Response('',{status:404})).mockResolvedValueOnce(new Response('{}',{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal;await getCutoutOriginal('owner','base /?',signal);expect(await getCutoutCorrection('owner','base /?',signal)).toBeNull();const recipe={custom:true,strokes:[{tool:'brush' as const,size:0.2,points:[[0.5,0.5] as [number,number]],at:null}],feather:0.01};await startCutoutCorrection('owner','base /?',recipe,signal);expect(fetch.mock.calls.map(call=>call[0])).toEqual(['/api/cutout/jobs/base%20%2F%3F/original','/api/cutout/jobs/base%20%2F%3F/correction','/api/cutout/jobs/base%20%2F%3F/corrections']);for(const call of fetch.mock.calls){expect(new Headers(call[1]?.headers).get('authorization')).toBe('Bearer owner');expect(call[1]?.signal).toBe(signal)}expect(JSON.parse(String(fetch.mock.calls[2]![1]?.body))).toEqual(recipe)
})

it('submits saved mask correction with explicit project and optional timeline scope',async()=>{
 const fetch=vi.fn().mockImplementation(()=>Promise.resolve(new Response('{"jobId":"edit"}',{headers:{'content-type':'application/json'}})));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal;const correction={custom:false,strokes:[],feather:0};
 await startProjectCutoutCorrection('owner','project /?','picture',correction,'timeline',signal);
 expect(fetch.mock.calls[0]?.[0]).toBe('/api/composition-projects/project%20%2F%3F/cutout/corrections');const options=fetch.mock.calls[0]?.[1];expect(options.signal).toBe(signal);expect(new Headers(options.headers).get('authorization')).toBe('Bearer owner');expect(JSON.parse(options.body)).toEqual({clipId:'picture',compositionId:'timeline',correction});
 await startProjectCutoutCorrection('owner','project','picture',correction);expect(JSON.parse(fetch.mock.calls[1]?.[1].body)).toEqual({clipId:'picture',correction});
})

it('reads project mask settings and bounded original preview with encoded timeline and owner signal',async()=>{
 const fetch=vi.fn().mockResolvedValueOnce(new Response('{}',{headers:{'content-type':'application/json'}})).mockResolvedValueOnce(new Response(new Uint8Array(64),{headers:{'content-type':'image/png'}}));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal;const context={projectId:'project /?',clipId:'clip /?',compositionId:'timeline /?',snapshot:{} as import('./composition/types').CutoutAttachment};await getProjectCutoutSettings('owner',context,signal);await getProjectCutoutOriginal('owner',context,signal);expect(fetch.mock.calls.map(call=>call[0])).toEqual(['/api/composition-projects/project%20%2F%3F/cutout/clip%20%2F%3F?compositionId=timeline%20%2F%3F','/api/composition-projects/project%20%2F%3F/cutout/clip%20%2F%3F/original?compositionId=timeline%20%2F%3F']);for(const call of fetch.mock.calls){expect(call[1]?.signal).toBe(signal);expect(new Headers(call[1]?.headers).get('authorization')).toBe('Bearer owner')}
})
it('sends Concat subject and revision without client paths or hashes',async()=>{
 const {analyseNestedCutout}=await import('./api')
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const signal=new AbortController().signal;await analyseNestedCutout('project /?','timeline','clip',7,'person',0.01,'owner',signal)
 expect(String(fetch.mock.calls[0]![0])).toBe('/api/composition-projects/project%20%2F%3F/concat-cutout')
 const options=fetch.mock.calls[0]![1]!;expect(options.signal).toBe(signal);expect(new Headers(options.headers).get('authorization')).toBe('Bearer owner');expect(JSON.parse(String(options.body))).toEqual({compositionId:'timeline',clipId:'clip',expectedRevision:7,cutout:{mode:'auto',subject:'person',feather:0.01}})
})
it('keeps manual brush recipe when repeating analysis with another subject',async()=>{
 const {analyseNestedCutout}=await import('./api')
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const correction={custom:true,feather:0.01,strokes:[{tool:'brush' as const,size:0.2,points:[[0.5,0.5] as [number,number]],at:null}]}
 await analyseNestedCutout('p','timeline','clip',2,'object',0.02,'owner',undefined,correction)
 expect(JSON.parse(String(fetch.mock.calls[0]![1]?.body)).cutout).toEqual({mode:'custom',subject:'object',feather:0.02,strokes:correction.strokes})
})
it('replays smart stroke prompts without sending client region attachments',async()=>{
 const {analyseNestedCutout}=await import('./api')
 const fetch=vi.fn(async(_input:RequestInfo|URL,_options?:RequestInit)=>new Response('{}',{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch)
 const region={sourceId:'raw',maskSourceId:'saved-region',sourceHash:'a'.repeat(64),maskHash:'b'.repeat(64),maskFileHash:'c'.repeat(64),encoderHash:'d'.repeat(64),decoderHash:'e'.repeat(64),runtimeHash:'f'.repeat(64),promptHash:'a'.repeat(64),points:[[0.5,0.5] as [number,number]]}
 await analyseNestedCutout('p','t','clip',2,'person',0.01,'owner',undefined,{custom:true,feather:0.01,strokes:[{tool:'smartBrush',size:0.2,points:region.points,at:0,region}]})
 expect(JSON.parse(String(fetch.mock.calls[0]![1]?.body)).cutout.strokes).toEqual([{tool:'smartBrush',size:0.2,points:region.points,at:0}])
})
