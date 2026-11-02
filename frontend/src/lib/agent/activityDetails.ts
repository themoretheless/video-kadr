/** Bound optional diagnostic details; never retain binary payloads or credentials. */
export function activityDetails(value:unknown):string {
 try {
 let budget=3500
 const seen=new WeakSet<object>(),bytes=(text:string)=>new TextEncoder().encode(text).length
 const clean=(text:string)=>text.replace(/(bearer\s+)[^\s"',;]+/gi,'$1[redacted]').replace(/((?:authorization|password|secret|access_token|token|(?:x-)?api[_-]?key)\s*[=:]\s*)["']?[^\s"',;&]+["']?/gi,'$1[redacted]')
 const walk=(input:unknown,depth:number):unknown=>{
  if(depth>5||budget<=0)return '[omitted]'
  budget-=16
  if(typeof input==='string'){
   const text=clean(input).slice(0,Math.min(512,Math.max(0,budget)))
   budget-=bytes(text);return text
  }
  if(!input||typeof input!=='object')return typeof input==='function'||typeof input==='symbol'?'[unsupported]':typeof input==='bigint'?String(input):input
  if(ArrayBuffer.isView(input)||input instanceof ArrayBuffer)return '[binary omitted]'
  if(seen.has(input))return '[circular]'
  seen.add(input)
  if(Array.isArray(input))return input.slice(0,30).map(item=>walk(item,depth+1))
  const result:Record<string,unknown>=Object.create(null)
  for(const [key,item] of Object.entries(input).slice(0,30)){
   if(budget<=0)break
   budget-=bytes(key)
   result[key.slice(0,128)]=/token|authorization|password|secret|api.?key/i.test(key)?'[redacted]':/base64|^rgba$|^pcm$|^bytes$|binary/i.test(key)?'[binary omitted]':walk(item,depth+1)
  }
  return result
 }
 const text=JSON.stringify(walk(value,0))??'null'
 return bytes(text)<=4096?text:JSON.stringify({truncated:true,preview:clean(text).slice(0,700)})
 } catch { return '"[unavailable]"' }
}

export function activityError(value:unknown):string {
 let message:string
 try {message=value instanceof Error?value.message:String(value)}catch{message='Unknown error'}
 const result:unknown=JSON.parse(activityDetails(message))
 return typeof result==='string'?result:'[omitted]'
}
