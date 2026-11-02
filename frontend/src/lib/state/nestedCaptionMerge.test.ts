import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {nestedComposition,nestedHistory,closeNestedComposition,resetNestedHistory,mergeNestedCaptionClips,undoNestedComposition} from './nestedComposition.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from './composition/core.svelte'
import {importSrtToComposition} from './composition/timeline.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
beforeEach(()=>{
 closeNestedComposition();resetNestedHistory();setCompositionStorageForTests(null);resetCompositionForTests()
 importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOne\n\n2\n00:00:03,000 --> 00:00:04,000\nTwo')
 const content=JSON.parse(JSON.stringify(buildCompositionWireDocument(compositionState.document)))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
 nestedComposition.activeCompositionId='root';nestedComposition.renderedOutputs={root:'old-movie'}
})
afterEach(()=>{vi.restoreAllMocks();closeNestedComposition()})
const ids=()=>nestedComposition.document!.compositions.root!.content.tracks[0]!.clips.map(clip=>clip.id)
it('validates once and commits one nested Undo with render cache restoration',async()=>{
 const before=JSON.stringify(nestedComposition.document)
 const validate=vi.spyOn(api,'nestedDurations').mockResolvedValue({root:4_000_000})
 await mergeNestedCaptionClips('root',ids())
 expect(validate).toHaveBeenCalledTimes(1)
 expect(nestedHistory.past).toHaveLength(1)
 expect(nestedComposition.renderedOutputs).toEqual({})
 expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips).toHaveLength(1)
 undoNestedComposition()
 expect(JSON.stringify(nestedComposition.document)).toBe(before)
 expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'})
})
it('does not overwrite content changed while native validation is pending',async()=>{
 let resolve!:(value:Record<string,number>)=>void
 vi.spyOn(api,'nestedDurations').mockImplementation(()=>new Promise(done=>{resolve=done}))
 const operation=mergeNestedCaptionClips('root',ids())
 const changed=JSON.parse(JSON.stringify(nestedComposition.document))
 changed.compositions.root.content.canvas.width=320
 nestedComposition.document=changed
 resolve({root:4_000_000});await operation
 expect(nestedComposition.document).toEqual(changed)
 expect(nestedHistory.past).toHaveLength(0)
 expect(nestedHistory.error).toContain('изменился')
 expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'})
})
it('refuses native validation failures and discards results after project close',async()=>{
 const before=JSON.stringify(nestedComposition.document)
 vi.spyOn(api,'nestedDurations').mockRejectedValueOnce(new Error('Native validation failed'))
 await mergeNestedCaptionClips('root',ids())
 expect(JSON.stringify(nestedComposition.document)).toBe(before)
 expect(nestedHistory.past).toHaveLength(0)
 expect(nestedHistory.error).toContain('Native validation failed')
 let resolve!:(value:Record<string,number>)=>void
 vi.mocked(api.nestedDurations).mockImplementation(()=>new Promise(done=>{resolve=done}))
 const operation=mergeNestedCaptionClips('root',ids())
 closeNestedComposition()
 resolve({root:4_000_000});await operation
 expect(nestedComposition.document).toBeNull()
 expect(nestedHistory.past).toHaveLength(0)
 expect(nestedHistory.error).toBe('')
})

it('imports overlapping subtitles with one native validation and one Undo',async()=>{
 const {importNestedCaptionFile}=await import('./nestedComposition.svelte')
 const before=JSON.stringify(nestedComposition.document)
 const validate=vi.spyOn(api,'nestedDurations').mockResolvedValue({root:4_000_000})
 expect(await importNestedCaptionFile('root','WEBVTT\n\n00:01.500 --> 00:02.500\nImported')).toBe(true)
 expect(validate).toHaveBeenCalledTimes(1)
 expect(nestedHistory.past).toHaveLength(1)
 expect(nestedComposition.document!.compositions.root!.content.tracks).toHaveLength(2)
 undoNestedComposition()
 expect(JSON.stringify(nestedComposition.document)).toBe(before)
 expect(nestedComposition.renderedOutputs).toEqual({root:'old-movie'})
})
it('refuses malformed or native-rejected subtitle imports without recording anything',async()=>{
 const {importNestedCaptionFile}=await import('./nestedComposition.svelte')
 const before=JSON.stringify(nestedComposition.document)
 const validate=vi.spyOn(api,'nestedDurations').mockRejectedValue(new Error('Native import refused'))
 expect(await importNestedCaptionFile('root','invalid')).toBe(false)
 expect(validate).not.toHaveBeenCalled()
 expect(await importNestedCaptionFile('root','1\n00:00:00,000 --> 00:00:01,000\nImport')).toBe(false)
 expect(nestedHistory.error).toContain('Native import refused')
 expect(nestedHistory.past).toHaveLength(0)
 expect(JSON.stringify(nestedComposition.document)).toBe(before)
})

it('edits a caption with native validation, one Undo and an exact stale-text guard',async()=>{
 const {updateNestedCaptionText}=await import('./nestedComposition.svelte')
 const before=JSON.stringify(nestedComposition.document),id=ids()[0]!
 const validate=vi.spyOn(api,'nestedDurations').mockResolvedValue({root:4_000_000})
 expect(await updateNestedCaptionText('root',id,'Исправлено','One')).toBe(true)
 expect(validate).toHaveBeenCalledTimes(1);expect(nestedHistory.past).toHaveLength(1)
 undoNestedComposition();expect(JSON.stringify(nestedComposition.document)).toBe(before)
 expect(await updateNestedCaptionText('root',id,'New','Stale')).toBe(false)
 expect(nestedHistory.error).toContain('изменился')
 expect(JSON.stringify(nestedComposition.document)).toBe(before)
})
