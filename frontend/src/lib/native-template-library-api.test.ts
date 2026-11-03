import {afterEach,expect,it,vi} from 'vitest'
import {listNativeTemplateLibrary,installNativeTemplate,removeNativeTemplate,createProjectFromNativeLibrary} from './api'
afterEach(()=>vi.unstubAllGlobals())
it('lists, installs and deletes only through authenticated owner endpoints',async()=>{
 const fetcher=vi.fn().mockResolvedValueOnce(new Response('[]')).mockResolvedValueOnce(new Response('{"id":"saved"}')).mockResolvedValueOnce(new Response(null,{status:204}));vi.stubGlobal('fetch',fetcher)
 expect(await listNativeTemplateLibrary('owner')).toEqual([]);const file=new File(['package'],'design.vktemplate');await installNativeTemplate(file,'owner');await removeNativeTemplate('saved/id','owner')
 expect(fetcher.mock.calls[0]![1].headers.Authorization).toBe('Bearer owner');expect(fetcher.mock.calls[1]![1].body.get('file')).toBe(file);expect(fetcher.mock.calls[2]![0]).toContain('/api/template-library/saved%2Fid');expect(fetcher.mock.calls[2]![1].method).toBe('DELETE')
})
it('creates from a stored identity without uploading package bytes and validates slot ownership',async()=>{
 const reply={project:{id:'new',schemaVersion:2,mode:'composition',document:{schemaVersion:1,root:'root',compositions:{root:{}},projectMedia:{slot:{librarySourceId:'chosen'}}}},sourceMapping:{}}
 const fetcher=vi.fn().mockResolvedValue(new Response(JSON.stringify(reply)));vi.stubGlobal('fetch',fetcher)
 await createProjectFromNativeLibrary('saved',[{mediaId:'slot',sourceId:'chosen'}],'owner','space');const[path,options]=fetcher.mock.calls[0]!;expect(String(path)).toContain('/api/template-library/saved/create?fills=');expect(options.body).toBeUndefined();expect(options.headers['X-Space-Id']).toBe('space')
 fetcher.mockResolvedValue(new Response(JSON.stringify(reply)));await expect(createProjectFromNativeLibrary('saved',[{mediaId:'slot',sourceId:'wrong'}],'owner')).rejects.toThrow('invalid project')
})
