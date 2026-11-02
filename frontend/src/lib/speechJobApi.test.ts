import {afterEach,expect,it,vi} from 'vitest'
import {transcribeLocalSpeech,observeLocalSpeechJob,listLocalSpeechJobs} from './api'
afterEach(()=>vi.unstubAllGlobals())
const input={sourceId:'speech',startUs:0,endUs:1000000,language:'en'}
const response=(data:unknown)=>new Response(JSON.stringify(data),{status:200,headers:{'Content-Type':'application/json'}})
it('admits durable speech and polls owner-authenticated result',async()=>{
  const transcript={language:'en',segments:[]}
  const fetch=vi.fn().mockResolvedValueOnce(response({jobId:'speech-job'})).mockResolvedValueOnce(response({status:'done',result:transcript}))
  vi.stubGlobal('fetch',fetch)
  expect(await transcribeLocalSpeech('owner-token',input)).toEqual(transcript)
  expect(fetch.mock.calls[0]![0]).toBe('/api/speech/jobs')
  expect(fetch.mock.calls[1]![0]).toBe('/api/jobs/speech-job')
  expect(new Headers(fetch.mock.calls[1]![1]?.headers).get('Authorization')).toBe('Bearer owner-token')
})
it('cancels admitted work even when abort happens before admission returns',async()=>{
  let admit!:(value:Response)=>void
  const fetch=vi.fn().mockImplementationOnce(()=>new Promise(resolve=>{admit=resolve})).mockResolvedValueOnce(response({status:'cancelled'}))
  vi.stubGlobal('fetch',fetch)
  const controller=new AbortController()
  const task=transcribeLocalSpeech('owner-token',input,controller.signal)
  controller.abort();admit(response({jobId:'late-job'}))
  await expect(task).rejects.toBeDefined()
  expect(fetch).toHaveBeenCalledTimes(2)
  expect(fetch.mock.calls[1]![0]).toBe('/api/jobs/late-job/cancel')
  expect(new Headers(fetch.mock.calls[1]![1]?.headers).get('Authorization')).toBe('Bearer owner-token')
})
it('reports a rejected cancellation instead of claiming the server stopped',async()=>{
  const controller=new AbortController()
  vi.stubGlobal('fetch',vi.fn().mockImplementationOnce(async()=>{controller.abort();return response({jobId:'job'})}).mockResolvedValueOnce(new Response('denied',{status:403})))
  await expect(transcribeLocalSpeech('owner-token',input,controller.signal)).rejects.toThrow('Не удалось подтвердить отмену')
})

it('forwards observed queue and backend progress without inventing progress on admission',async()=>{
  const fetch=vi.fn().mockResolvedValueOnce(response({jobId:'progress-job'})).mockResolvedValueOnce(response({status:'done',progress:100,result:{segments:[]}}))
  vi.stubGlobal('fetch',fetch)
  const observe=vi.fn()
  await transcribeLocalSpeech('owner',input,undefined,observe)
  expect(observe.mock.calls.map(([update])=>update)).toEqual([{jobId:'progress-job',status:'pending'},{jobId:'progress-job',status:'done',progress:100}])
})

it('keeps server work after observation failure and reconnects with GET only',async()=>{
  const fetch=vi.fn().mockResolvedValueOnce(response({jobId:'persistent'})).mockResolvedValueOnce(new Response('offline',{status:503}))
  vi.stubGlobal('fetch',fetch)
  await expect(transcribeLocalSpeech('owner',input)).rejects.toBeDefined()
  expect(fetch).toHaveBeenCalledTimes(2)
  fetch.mockResolvedValueOnce(response({status:'done',result:{language:'en',segments:[]}}))
  expect(await observeLocalSpeechJob('owner','persistent')).toEqual({language:'en',segments:[]})
  expect(fetch.mock.calls.map(([url])=>url)).toEqual(['/api/speech/jobs','/api/jobs/persistent','/api/jobs/persistent'])
})
it('does not retry or cancel a terminal failed job while reconnecting',async()=>{
  const fetch=vi.fn().mockResolvedValueOnce(response({status:'error',error:'model failed'}))
  vi.stubGlobal('fetch',fetch)
  await expect(observeLocalSpeechJob('owner','failed')).rejects.toThrow('model failed')
  expect(fetch).toHaveBeenCalledTimes(1)
})

it('loads owner-scoped history with an encoded page cursor',async()=>{
  const page={jobs:[],nextCursor:null}
  const fetch=vi.fn().mockResolvedValueOnce(response(page));vi.stubGlobal('fetch',fetch)
  expect(await listLocalSpeechJobs('owner-token','a/b')).toEqual(page)
  expect(fetch.mock.calls[0]![0]).toBe('/api/speech/jobs?cursor=a%2Fb')
  expect(new Headers(fetch.mock.calls[0]![1]?.headers).get('Authorization')).toBe('Bearer owner-token')
})
