<script lang="ts">
 import Manager from '../SpeechModelManager.svelte'
 let {onchanged}:{onchanged:()=>void}=$props()
 let token=$state('owner')
</script>
<button onclick={()=>token='other'}>Сменить аккаунт</button>
<Manager {token} {onchanged}/>
