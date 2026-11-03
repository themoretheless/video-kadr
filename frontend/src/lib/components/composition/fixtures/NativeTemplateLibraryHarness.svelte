<script lang="ts">
 import Control from '../NativeTemplateLibrary.svelte'
 let token=$state('owner')
 export function changeAccount(){token='other'}
</script>
<Control {token} context="document" oncreated={async()=>{}}/>
