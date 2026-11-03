<script lang="ts">
 import Control from '../NativeTemplateImport.svelte'
 import type {NativeTemplateImportResponse} from '$lib/api'
 let {oncreated}:{oncreated:(result:NativeTemplateImportResponse,guard:()=>void)=>Promise<void>}=$props()
 let token=$state('owner')
 export function changeAccount(){token='other'}
</script>
<Control {token} context="document" {oncreated}/>
