<script lang="ts">
 import Panel from '../TextToSpeechPanel.svelte'
 let token=$state('owner')
</script>
<button onclick={()=>token='other'}>Сменить владельца озвучки</button>
<Panel {token}/>
