export interface SearchableGraphicTemplate {readonly name?:unknown;readonly category?:unknown;readonly description?:unknown;readonly tags?:unknown}
export function matchesGraphicTemplate(template:SearchableGraphicTemplate,query:string,category:string):boolean {
 const text=(value:unknown)=>typeof value==='string'?value.toLowerCase():''
 if(category && text(template.category)!==category.toLowerCase())return false
 const fields=[template.name,template.category,template.description,...(Array.isArray(template.tags)?template.tags:[])]
 return !query || fields.some(value=>text(value).includes(query.toLowerCase()))
}
export function graphicTemplateCategories(templates:readonly SearchableGraphicTemplate[]):string[]{
 return [...new Set(templates.flatMap(template=>typeof template.category==='string'&&template.category?[template.category]:[]))].sort((a,b)=>a.localeCompare(b))
}
