import {afterEach,expect,it,vi} from 'vitest'
import {analyseNestedVideoCutout,previewNestedVideoCutout,setNestedVideoCutoutBinding,startNestedVideoSmartRegion,attachNestedVideoSmartRegion,observeNestedVideoSmartRegion,cancelNestedVideoSmartRegion,getNestedVideoSmartOperation,cancelNestedVideoSmartOperation} from './api'
afterEach(()=>{vi.unstubAllGlobals();vi.useRealTimers()})
it('sends an explicit separately cancellable operation after abort through a proxy',async()=>{
 const fetchMock=vi.fn((path:string,init?:RequestInit):Promise<Response>=>{
  if(path.endsWith('/cancel'))return Promise.resolve(new Response(null,{status:204}))
  return new Promise((_resolve,reject)=>init!.signal!.addEventListener('abort',()=>reject(new DOMException('cancelled','AbortError')),{once:true}))
 });vi.stubGlobal('fetch',fetchMock);const cancel=new AbortController();const pending=analyseNestedVideoCutout('saved','root','clip',7,'owner',cancel.signal,'auto').catch(error=>error);expect(fetchMock).toHaveBeenCalledTimes(1);const body=JSON.parse(String(fetchMock.mock.calls[0]![1]!.body));expect(body).toMatchObject({compositionId:'root',clipId:'clip',expectedRevision:7,subject:'auto'});expect(body.operationId).toMatch(/^[0-9a-f-]{36}$/);cancel.abort();await pending;expect(fetchMock).toHaveBeenCalledTimes(2);const[url,options]=fetchMock.mock.calls[1]!;expect(url).toBe('/api/composition-projects/saved/concat-video-cutout/cancel');expect(JSON.parse(String(options!.body))).toEqual({operationId:body.operationId});expect(new Headers(options!.headers).get('authorization')).toBe('Bearer owner');expect(options!.keepalive).toBe(true);expect(options!.signal).not.toBe(cancel.signal);expect(options!.signal!.aborted).toBe(false)
})
it('does not leave an abort hook after successful analysis or send pre-cancelled work',async()=>{
 const report={sourceId:'source',analysedFrames:1,analysedRuns:1,emptyRuns:0,cacheReused:false};const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(report),{status:200}));vi.stubGlobal('fetch',fetchMock);const cancel=new AbortController();expect(await analyseNestedVideoCutout('saved','root','clip',7,'owner',cancel.signal)).toEqual(report);cancel.abort();expect(fetchMock).toHaveBeenCalledTimes(1);await expect(analyseNestedVideoCutout('saved','root','clip',7,'owner',cancel.signal)).rejects.toMatchObject({name:'AbortError'});expect(fetchMock).toHaveBeenCalledTimes(1)
})

it('transports authored video strokes for preview and saved activation',async()=>{
 const correction={custom:true,feather:0.01,strokes:[{tool:'brush' as const,size:0.1,points:[[0.25,0.5] as [number,number]],at:null}]};
 const fetchMock=vi.fn().mockResolvedValueOnce(new Response(new Uint8Array([1,2,3,4]),{headers:{'x-frame-width':'1','x-frame-height':'1'}})).mockResolvedValueOnce(new Response('{}',{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetchMock);
 expect((await previewNestedVideoCutout('saved','root','clip',7,0,'owner',undefined,0.01,correction,true,true)).rgba).toEqual(new Uint8ClampedArray([1,2,3,4]));
 await setNestedVideoCutoutBinding('saved','root','clip',7,true,'owner',undefined,'person','a'.repeat(64),0.01,correction);
 for(const [,options] of fetchMock.mock.calls)expect(JSON.parse(String(options.body)).correction).toEqual(correction);
 expect(JSON.parse(String(fetchMock.mock.calls[0]![1].body)).sourceSpace).toBe(true);expect(JSON.parse(String(fetchMock.mock.calls[0]![1].body)).sourceOriginal).toBe(true);
})

it('transports explicit temporal track removal rather than inheriting saved regions',async()=>{
 const correction={custom:false,feather:0,strokes:[]};const fetchMock=vi.fn().mockResolvedValueOnce(new Response(new Uint8Array(4),{headers:{'x-frame-width':'1','x-frame-height':'1'}})).mockResolvedValueOnce(new Response('{}'));vi.stubGlobal('fetch',fetchMock)
 await previewNestedVideoCutout('saved','root','clip',7,0,'owner',undefined,0,correction,true,undefined,[])
 await setNestedVideoCutoutBinding('saved','root','clip',7,true,'owner',undefined,undefined,undefined,0,correction,[])
 for(const [,options]of fetchMock.mock.calls)expect(JSON.parse(String(options.body))).toMatchObject({regionTracks:[],correction})
})

it('starts a saved smart region from the cursor without accepting host paths or seed ordinals',async()=>{
 const accepted={jobId:'job',sourceId:'source',sourceUs:100000,seedFrame:1,frames:3,frameRate:{numerator:10,denominator:1},expectedRevision:7};const fetchMock=vi.fn().mockResolvedValue(new Response(JSON.stringify(accepted)));vi.stubGlobal('fetch',fetchMock)
 expect(await startNestedVideoSmartRegion('saved','root','clip',7,50000,[[.5,.5]],'owner')).toEqual(accepted);expect(fetchMock.mock.calls[0]![0]).toBe('/api/composition-projects/saved/concat-video-cutout/smart-region');expect(JSON.parse(String(fetchMock.mock.calls[0]![1].body))).toEqual({compositionId:'root',clipId:'clip',expectedRevision:7,timeUs:50000,points:[[.5,.5]]})
})

it('attaches a finished smart job using a revisioned target and server-owned result',async()=>{
 const fetchMock=vi.fn().mockResolvedValue(new Response('{}'));vi.stubGlobal('fetch',fetchMock)
 await attachNestedVideoSmartRegion('saved','root','clip',8,50000,'job','smartEraser',.1,'owner')
 expect(fetchMock.mock.calls[0]![0]).toBe('/api/composition-projects/saved/concat-video-cutout/smart-region/attach');expect(JSON.parse(String(fetchMock.mock.calls[0]![1].body))).toEqual({compositionId:'root',clipId:'clip',expectedRevision:8,timeUs:50000,jobId:'job',tool:'smartEraser',size:.1})
})

it('observes owned smart work with progress and cleans its polling timer',async()=>{
 vi.useFakeTimers();const progress=vi.fn(),fetchMock=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({id:'job',status:'running',progress:30}))).mockResolvedValueOnce(new Response(JSON.stringify({id:'job',status:'done'})));vi.stubGlobal('fetch',fetchMock)
 const pending=observeNestedVideoSmartRegion('owner','job',undefined,progress);await vi.advanceTimersByTimeAsync(500);expect((await pending).status).toBe('done');expect(progress).toHaveBeenCalledTimes(2);expect(vi.getTimerCount()).toBe(0);expect(new Headers(fetchMock.mock.calls[0]![1].headers).get('authorization')).toBe('Bearer owner')
})
it('stops smart observation on abort and only cancels with an explicit owned request',async()=>{
 vi.useFakeTimers();const fetchMock=vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({id:'job',status:'running'}))).mockResolvedValueOnce(new Response('{}'));vi.stubGlobal('fetch',fetchMock);const signal=new AbortController();const pending=observeNestedVideoSmartRegion('owner','job',signal.signal).catch(error=>error);await vi.advanceTimersByTimeAsync(0);signal.abort();expect(await pending).toMatchObject({name:'AbortError'});expect(vi.getTimerCount()).toBe(0);expect(fetchMock).toHaveBeenCalledTimes(1)
 await cancelNestedVideoSmartRegion('owner','job');expect(fetchMock.mock.calls[1]![0]).toBe('/api/jobs/job/cancel');expect(fetchMock.mock.calls[1]![1].method).toBe('POST');expect(fetchMock.mock.calls[1]![1].signal.aborted).toBe(false)
})

it('keeps an operation ID across smart-start retries and uses authenticated lookup/cancel routes',async()=>{
 const operation='11111111-1111-4111-8111-111111111111',fetchMock=vi.fn().mockImplementation(()=>Promise.resolve(new Response('{}')));vi.stubGlobal('fetch',fetchMock)
 for(let attempt=0;attempt<2;attempt++)await startNestedVideoSmartRegion('saved','root','clip',7,0,[[.5,.5]],'owner',undefined,operation)
 expect(fetchMock.mock.calls.slice(0,2).map(call=>JSON.parse(String(call[1].body)).operationId)).toEqual([operation,operation])
 await getNestedVideoSmartOperation('saved',operation,'owner');await cancelNestedVideoSmartOperation('saved',operation,'owner')
 expect(fetchMock.mock.calls[2]![0]).toBe(`/api/composition-projects/saved/concat-video-cutout/smart-region/operations/${operation}`)
 expect(fetchMock.mock.calls[3]![0]).toBe(`/api/composition-projects/saved/concat-video-cutout/smart-region/operations/${operation}/cancel`)
 for(const [,init]of fetchMock.mock.calls)expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner')
 expect(fetchMock.mock.calls[3]![1].signal.aborted).toBe(false)
})
