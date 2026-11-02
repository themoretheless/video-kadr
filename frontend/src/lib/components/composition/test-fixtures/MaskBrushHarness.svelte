<script lang="ts">
 import Editor from '../MaskBrushEditor.svelte'
 let {onready,onchanged}:{onready:(id:string)=>void;onchanged:(dirty:boolean)=>void}=$props()
 let token=$state('owner'),jobId=$state('base')
</script>
<button onclick={()=>token='other'}>Сменить аккаунт кисти</button><button onclick={()=>jobId='another'}>Сменить маску</button>
<Editor {token} {jobId} width={37} height={23} {onready} {onchanged}/>
