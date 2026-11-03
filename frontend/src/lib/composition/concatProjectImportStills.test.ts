import {expect,it} from 'vitest'
import {concatImportStillDestinations,savedStillChoices,concatImportVideoDestinations,savedVideoChoices} from './concatProjectImport'
it('keeps same-named clips separate across destination timelines and supports flat files',()=>{
 const clip={id:'same',mediaId:'slot',kind:'image',cutout:{mode:'auto'}}
 const destinations=concatImportStillDestinations({timelines:[{id:'a',clips:[clip]},{id:'b',clips:[clip,{...clip,id:'plain',cutout:null}]}]})
 expect(destinations.map(v=>[v.timelineId,v.clipId])).toEqual([['a','same'],['b','same']]);expect(new Set(destinations.map(v=>v.key)).size).toBe(2)
 expect(concatImportStillDestinations({clips:[clip]})[0]).toMatchObject({timelineId:'TL1',clipId:'same',mediaId:'slot'})
})
it('finds flat and nested saved masks and resolves a logical original through its library binding',()=>{
 const content={sources:{logical:{id:'logical',kind:'image'},copy:{id:'copy',kind:'image'}},metadata:{mediaVersions:{same:{originalSourceId:'logical',copySourceId:'copy',cutout:{}}}},tracks:[{kind:'image',clips:[{id:'same',sourceId:'copy'}]}]}
 const rows=savedStillChoices([{id:'flat',name:'Flat',revision:3,document:content},{id:'nested',name:'Nested',revision:7,document:{projectMedia:{logical:{librarySourceId:'physical'}},compositions:{a:{content},b:{content}}}}])
 expect(rows.map(row=>[row.sourceId,row.reference.compositionId,row.reference.expectedRevision])).toEqual([['logical',null,3],['physical','a',7],['physical','b',7]])
 expect(new Set(rows.map(row=>row.key)).size).toBe(3)
})
it('does not offer absent originals, unrelated selected copies or invalid saved revisions',()=>{
 const content={sources:{},metadata:{mediaVersions:{same:{originalSourceId:'original',copySourceId:'copy',cutout:{}}}},tracks:[{kind:'image',clips:[{id:'same',sourceId:'other'}]}]}
 expect(savedStillChoices([{id:'bad',name:'Bad',revision:1,document:content},{id:'invalid',name:'Invalid',revision:NaN,document:{}},{id:'empty',name:'Empty',revision:1,document:null}])).toEqual([])
})

it('keeps video destination scopes and offers only saved owned video stores',()=>{
 const clip={id:'same',mediaId:'slot',kind:'video',cutout:{mode:'auto'}}
 expect(concatImportVideoDestinations({timelines:[{id:'a',clips:[clip]},{id:'b',clips:[clip]}]}).map(v=>[v.timelineId,v.clipId])).toEqual([['a','same'],['b','same']])
 expect(concatImportVideoDestinations({clips:[clip,{...clip,id:'image',kind:'image'}]})).toHaveLength(1)
 const content={sources:{slot:{id:'slot',kind:'video'}},tracks:[{kind:'video',clips:[{id:'owned',sourceId:'slot',concatCutout:{}},{id:'draft',sourceId:'slot',concatCutout:{origin:{projectId:'elsewhere'}}},{id:'plain',sourceId:'slot'},{id:'missing',sourceId:'missing',concatCutout:{}}]}]}
 const projects=[{id:'flat',name:'Flat',revision:1,document:content},{id:'nested',name:'Nested',revision:7,document:{projectMedia:{slot:{librarySourceId:'physical'}},compositions:{a:{content},b:{content}}}},{id:'bad',name:'Bad',revision:0,document:content}]
 const choices=savedVideoChoices(projects)
 expect(choices.map(c=>[c.sourceId,c.reference.compositionId,c.reference.sourceClipId])).toEqual([['slot',null,'owned'],['physical','a','owned'],['physical','b','owned']]);expect(new Set(choices.map(c=>c.key)).size).toBe(3)
})
