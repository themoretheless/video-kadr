import {expect,it} from 'vitest'
import {concatImportFonts} from './concatProjectImport'
it('keeps source paths as opaque binding keys including prototype-like names',()=>{
 expect(concatImportFonts({})).toEqual([])
 expect(concatImportFonts({fonts:[{family:'Custom',path:'/never/open/font.ttf'},{family:'Second',path:'__proto__'}]})).toEqual([{family:'Custom',path:'/never/open/font.ttf'},{family:'Second',path:'__proto__'}])
})
it('rejects duplicate or malformed font declarations and the file-count budget',()=>{
 for(const fonts of [null,{},[null],[{family:'',path:'font'}],[{family:'Custom',path:''}],[{family:'Custom',path:'a'},{family:'Other',path:'a'}],Array.from({length:33},(_,i)=>({family:'Custom',path:`${i}.ttf`}))]){
  if(fonts===null)expect(concatImportFonts({fonts})).toEqual([])
  else expect(()=>concatImportFonts({fonts})).toThrow()
 }
})
