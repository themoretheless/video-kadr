import {it,expect} from 'vitest'
import {matchesGraphicTemplate,graphicTemplateCategories} from './graphicTemplateSearch'
it('searches names categories descriptions and tags with exact category intersection',()=>{
 const template={name:'Титры',category:'Titles',description:'A lower third',tags:['Broadcast','новости']}
 for(const query of ['ТИТРЫ','titles','LOWER THIRD','broadcast','НОВОСТИ'])expect(matchesGraphicTemplate(template,query,'titles')).toBe(true)
 expect(matchesGraphicTemplate(template,'broadcast','Other')).toBe(false)
 expect(matchesGraphicTemplate(template,'missing','')).toBe(false)
 expect(matchesGraphicTemplate({},'','')).toBe(true)
 expect(graphicTemplateCategories([template,template,{category:'Callouts'},{}])).toEqual(['Callouts','Titles'])
})
