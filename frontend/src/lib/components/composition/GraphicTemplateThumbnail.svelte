<script lang="ts">
 import {getGraphicTemplateThumbnail,getGraphicTemplateDocumentThumbnail,type NativeGraphicTemplate,type GraphicTemplateThumbnail} from '$lib/api'
 let {template,document,name}:{template?:NativeGraphicTemplate;document?:Readonly<Record<string,unknown>>;name:string}=$props()
 let thumbnail=$state<GraphicTemplateThumbnail|null>(null),error=$state('')
 $effect(()=>{
  const selected=template,raw=document
  thumbnail=null;error=''
  if(!selected&&!raw)return
  const controller=new AbortController()
  const pending=selected?getGraphicTemplateThumbnail(selected,160,controller.signal):getGraphicTemplateDocumentThumbnail(raw!,160,controller.signal)
  void pending.then(result=>{if(!controller.signal.aborted)thumbnail=result}).catch(reason=>{if(!controller.signal.aborted)error=String(reason)})
  return ()=>controller.abort()
 })
</script>
{#if thumbnail}<img src={`data:image/png;base64,${thumbnail.pngBase64}`} width={thumbnail.width} height={thumbnail.height} alt={`Предпросмотр ${name}`} />{/if}
{#if error}<p role="alert">{error}</p>{/if}
<style>img{display:block;max-width:100%;height:auto;background:repeating-conic-gradient(#242830 0% 25%,#191d24 0% 50%) 0/12px 12px}p{font-size:.75rem;overflow-wrap:anywhere}</style>
