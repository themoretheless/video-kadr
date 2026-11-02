<script lang="ts">
 import Panel from '../NativeSpeechPanel.svelte'
 let token=$state('test-token')
</script>
<button onclick={()=>token='other-owner'}>Сменить аккаунт</button>
<Panel {token}/>
