import {afterEach,expect,it,vi} from 'vitest'
import {saveSamVideoRegion,startSamVideoTracking,listSamVideoTracking,getSamVideoPoints,saveSamRegion,getSamStatus,startSamRegion,listSamRegions,getSamPoints,getSamMask} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('uses authenticated cancellable bounded SAM transports without client model paths',async()=>{
 const fetch=vi.fn().mockImplementation(()=>Promise.resolve(new Response('{}',{headers:{'content-type':'application/json'}})));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal;await getSamStatus('owner',signal);await startSamRegion('owner','source',[[-0.5,1.5],[0.5,0.5]],signal);await listSamRegions('owner','source /?','cursor /?',signal);await getSamPoints('owner','job /?',signal);
 expect(fetch.mock.calls.map(call=>call[0])).toEqual(['/api/sam/status','/api/sam/jobs','/api/sam/jobs?sourceId=source+%2F%3F&cursor=cursor+%2F%3F','/api/sam/jobs/job%20%2F%3F/points']);expect(JSON.parse(fetch.mock.calls[1]?.[1].body)).toEqual({sourceId:'source',points:[[-0.5,1.5],[0.5,0.5]]});for(const call of fetch.mock.calls){expect(new Headers(call[1].headers).get('authorization')).toBe('Bearer owner');expect(call[1].signal).toBe(signal)}
})
it('accepts a private PNG and refuses non-PNG or oversized masks',async()=>{
 const fetch=vi.fn().mockResolvedValueOnce(new Response(new Uint8Array(64),{headers:{'content-type':'image/png'}})).mockResolvedValueOnce(new Response('bad',{headers:{'content-type':'text/plain'}})).mockResolvedValueOnce(new Response(new Uint8Array(1024*1024+1),{headers:{'content-type':'image/png'}}));vi.stubGlobal('fetch',fetch);expect((await getSamMask('owner','job /?')).size).toBe(64);expect(fetch.mock.calls[0]?.[0]).toBe('/api/sam/jobs/job%20%2F%3F/mask');await expect(getSamMask('owner','job')).rejects.toThrow('PNG');await expect(getSamMask('owner','job')).rejects.toThrow('размер')
})

it('accepts a learned region explicitly into a project with owner identity and cancellation',async()=>{
 const asset={id:'mask',region:{maskSourceId:'mask',sourceId:'input'},source:{id:'input'}};const fetch=vi.fn().mockResolvedValue(new Response(JSON.stringify(asset),{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal;expect(await saveSamRegion('owner','job /?','project',signal)).toEqual(asset);expect(fetch.mock.calls[0]?.[0]).toBe('/api/sam/jobs/job%20%2F%3F/asset');const options=fetch.mock.calls[0]?.[1];expect(options.signal).toBe(signal);expect(new Headers(options.headers).get('authorization')).toBe('Bearer owner');expect(JSON.parse(options.body)).toEqual({projectId:'project'})
})

it('submits source ordinal tracking and requests video history separately from still regions',async()=>{
 const fetch=vi.fn().mockImplementation(()=>Promise.resolve(new Response('{}',{headers:{'content-type':'application/json'}})));vi.stubGlobal('fetch',fetch)
 const signal=new AbortController().signal
 await startSamVideoTracking('owner','source /?',[[0.5,0.5]],1,signal)
 await listSamVideoTracking('owner','source /?','cursor /?',signal)
 await getSamVideoPoints('owner','job /?',signal)
 expect(fetch.mock.calls.map(call=>call[0])).toEqual(['/api/sam/jobs','/api/sam/jobs?video=true&sourceId=source+%2F%3F&cursor=cursor+%2F%3F','/api/sam/jobs/job%20%2F%3F/points'])
 expect(JSON.parse(fetch.mock.calls[0]?.[1].body)).toEqual({sourceId:'source /?',points:[[0.5,0.5]],seedFrame:1})
 for(const call of fetch.mock.calls){expect(new Headers(call[1].headers).get('authorization')).toBe('Bearer owner');expect(call[1].signal).toBe(signal)}
 for(const ordinal of [-1,1.5,360000,NaN,Infinity])expect(()=>startSamVideoTracking('owner','source',[[0.5,0.5]],ordinal)).toThrow('кадра')
 expect(fetch).toHaveBeenCalledTimes(3)
})

it('explicitly accepts temporal regions into an authorized project with cancellation',async()=>{
 const fetch=vi.fn().mockResolvedValue(new Response('{}',{headers:{'content-type':'application/json'}}));vi.stubGlobal('fetch',fetch);const signal=new AbortController().signal
 await saveSamVideoRegion('owner','job /?','project',signal)
 expect(fetch.mock.calls[0]?.[0]).toBe('/api/sam/jobs/job%20%2F%3F/video-asset');expect(JSON.parse(fetch.mock.calls[0]?.[1].body)).toEqual({projectId:'project'});expect(fetch.mock.calls[0]?.[1].signal).toBe(signal);expect(new Headers(fetch.mock.calls[0]?.[1].headers).get('authorization')).toBe('Bearer owner')
})
