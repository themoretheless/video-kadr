import {afterEach,expect,it,vi} from 'vitest'
import {continueVideoWithRegion,getVideoRegionFrame} from './api'
afterEach(()=>vi.unstubAllGlobals())
function response(hash='a'.repeat(64),frame='1',type='image/png',bytes=64){return new Response(new Uint8Array(bytes),{headers:{'content-type':type,'x-mask-sha256':hash,'x-source-frame':frame}})}
it('reads private PNG/plane hash with exact ordinal, escaped IDs and cancellation',async()=>{
 const fetch=vi.fn().mockResolvedValue(response());vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal
 const data=await getVideoRegionFrame('owner','job /?','region /?',1,signal);expect(data.maskHash).toBe('a'.repeat(64));expect(data.png.size).toBe(64);expect(fetch.mock.calls[0]![0]).toBe('/api/person/video-jobs/job%20%2F%3F/regions/region%20%2F%3F/1');expect(new Headers(fetch.mock.calls[0]![1].headers).get('authorization')).toBe('Bearer owner');expect(fetch.mock.calls[0]![1].signal).toBe(signal)
 for(const ordinal of [-1,1.5,360000,NaN])await expect(getVideoRegionFrame('owner','job','region',ordinal)).rejects.toThrow('кадр')
 expect(fetch).toHaveBeenCalledTimes(1)
})
it('refuses non-PNG, missing/malformed identities, wrong frames and oversized bodies',async()=>{
 const fetch=vi.fn().mockResolvedValueOnce(response('a'.repeat(64),'1','text/plain')).mockResolvedValueOnce(response('','1')).mockResolvedValueOnce(response('A'.repeat(64),'1')).mockResolvedValueOnce(response('a'.repeat(64),'2')).mockResolvedValueOnce(response('a'.repeat(64),'1','image/png',1024*1024+1));vi.stubGlobal('fetch',fetch)
 for(let i=0;i<5;i++)await expect(getVideoRegionFrame('owner','job','region',1)).rejects.toThrow()
})

it('forks a region session through bearer POST without client filesystem paths or project rewrites',async()=>{
 const fetch=vi.fn().mockResolvedValue(new Response(JSON.stringify({jobId:'next'}),{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal
 expect(await continueVideoWithRegion('owner','job /?','tracked-job',[],signal)).toEqual({jobId:'next'})
 const [url,init]=fetch.mock.calls[0]!;expect(url).toBe('/api/person/video-jobs/job%20%2F%3F/regions');expect(init.method).toBe('POST');expect(init.signal).toBe(signal);expect(new Headers(init.headers).get('authorization')).toBe('Bearer owner');expect(JSON.parse(init.body)).toEqual({samJobId:'tracked-job',corrections:[]})
})
