<script lang="ts">
 import Editor from '../VideoMaskBrushEditor.svelte'
 let{onready,onchanged}:{onready:(id:string)=>void;onchanged:(dirty:boolean)=>void}=$props()
 let token=$state('owner'),jobId=$state('base')
</script>
<button onclick={()=>token='other'}>Сменить владельца видео</button><button onclick={()=>jobId='another'}>Сменить задание видео</button>
<Editor {token} {jobId} width={37} height={23} frames={4} {onready} {onchanged}/>
