import {afterEach,expect,it,vi} from 'vitest'
import {importNativeTemplate} from './api'
afterEach(()=>vi.unstubAllGlobals())
const source={id:'slot',kind:'image',durationTicks:0,width:16,height:8,hasAudio:false}
const reply={project:{id:'saved',schemaVersion:2,mode:'composition',document:{schemaVersion:1,root:'root',compositions:{root:{content:{},instances:[]}},projectMedia:{slot:{source,librarySourceId:'chosen',name:'Picture'}}}},sourceMapping:{'template-asset-0000':'fresh-design'}}
it('uploads only package bytes and slot identities with the target Space and session',async()=>{
 const fetcher=vi.fn().mockResolvedValue(new Response(JSON.stringify(reply)));vi.stubGlobal('fetch',fetcher)
 const file=new File(['package'],'edit.vktemplate'),fills=[{mediaId:'slot',sourceId:'chosen'}],controller=new AbortController()
 expect(await importNativeTemplate(file,fills,'session','space',controller.signal)).toEqual(reply)
 const[path,options]=fetcher.mock.calls[0]!;expect(JSON.parse(new URL(String(path),'http://local').searchParams.get('fills')!)).toEqual(fills)
 expect(options.headers.Authorization).toBe('Bearer session');expect(options.headers['X-Space-Id']).toBe('space');expect(options.signal).toBe(controller.signal)
 expect(options.body.get('file')).toBe(file);expect(options.headers['Content-Type']).toBeUndefined()
})
it('refuses duplicate slots before network and mismatched or unfinished server documents',async()=>{
 const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);const file=new File(['package'],'edit.vktemplate')
 await expect(importNativeTemplate(file,[{mediaId:'slot',sourceId:'a'},{mediaId:'slot',sourceId:'b'}],'session')).rejects.toThrow('Некорректные');expect(fetcher).not.toHaveBeenCalled()
 for(const item of [{...reply.project.document.projectMedia.slot,librarySourceId:'wrong'},{...reply.project.document.projectMedia.slot,placeholder:true}]){
  fetcher.mockResolvedValue(new Response(JSON.stringify({...reply,project:{...reply.project,document:{...reply.project.document,projectMedia:{slot:item}}}})))
  await expect(importNativeTemplate(file,[{mediaId:'slot',sourceId:'chosen'}],'session')).rejects.toThrow('invalid project')
 }
})
