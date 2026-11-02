<script lang="ts">
  import {SvelteDate} from 'svelte/reactivity'
  import {agentConnections,updateAgentConnections} from './connections.svelte'
  import AgentActivityPanel from './AgentActivityPanel.svelte'
  import { authState } from '../state/auth.svelte.js'
  import { spacesState } from '../state/spaces.svelte.js'
  import { agentSession, releaseAgentConnection, endAgentWork } from './session.svelte.js'
  import { executeAgentRequest } from './runtime.svelte.js'

  import {nestedComposition} from '../state/nestedComposition.svelte'
  import {visibleNestedCheckpoints} from './nestedCheckpoints.svelte'
  const checkpoints=$derived(nestedComposition.document?visibleNestedCheckpoints():agentSession.checkpoints)

  let token = $state('')
  let connected = $state(false)
  let error = $state('')
  let label = $state('')
  let expanded=$state(false)
  $effect(()=>{if(agentSession.view==='agent')expanded=true})
  let clock = $state(Date.now())
  const editorId = crypto.randomUUID()
  const activeLock = $derived(agentSession.lock && agentSession.lock.expiresAt > clock ? agentSession.lock : null)

  async function act(tool: string, args: Record<string, unknown>): Promise<void> {
    try {
      error = ''
      if(nestedComposition.document&&tool==='restore_checkpoint'){
        const snapshot=await executeAgentRequest('user','get_project',{}) as {projectHash:string}
        args={...args,expectedProjectHash:snapshot.projectHash}
      }
      await executeAgentRequest('user',tool,args)
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }

  $effect(() => {
    if (!connected) return
    const credential = token.trim()
    const controller = new AbortController()
    let stopped = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let heartbeatTimer: ReturnType<typeof setTimeout> | undefined
    async function request(path: string, body: unknown): Promise<Response> {
      return fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${credential}` }, body: JSON.stringify(body), signal: controller.signal })
    }
    let running=0
    async function processCommand(command:{id:string;actor:string;tool:string;args:Record<string,unknown>;expiresAt:number}):Promise<void> {
      running++
      try {
          let result: unknown
          let failure: string | undefined
          try {
            if (command.expiresAt <= Date.now()) throw new Error('Команда истекла; проект не изменён')
            const ownerToken = authState.token
            const ownerSpace = spacesState.selectedId
            result = await executeAgentRequest(command.actor, command.tool, command.args, { token: ownerToken, spaceId: ownerSpace, expiresAt: command.expiresAt, isActive: () => !stopped && authState.token === ownerToken && spacesState.selectedId === ownerSpace })
          } catch (cause) { failure = cause instanceof Error ? cause.message : String(cause) }
          if(stopped)return
          const acknowledgement = await request('/api/agent/result', { editorId, id: command.id, ...(failure ? { error: failure } : { result }) })
          if (!acknowledgement.ok) {
            if(acknowledgement.status===410)error='Запрос уже закрыт; действие могло завершиться. Проверьте документ.'
            else throw new Error('Не удалось подтвердить действие; проверьте журнал перед повтором')
          }
      } catch(cause) {
        if(!stopped){error=cause instanceof Error?cause.message:String(cause);connected=false}
      } finally {running--}
    }
    async function poll(): Promise<void> {
      if(stopped)return
      if(running>=32){timer=setTimeout(()=>void poll(),400);return}
      try {
        const response = await request('/api/agent/poll', { editorId })
        if (!response.ok) throw new Error(response.status === 409 ? 'Другой редактор уже подключён' : `Подключение отклонено (${response.status})`)
        const { command, releasedActors, connections } = await response.json()
        if (stopped) return
        updateAgentConnections(connections)
        for(const actor of releasedActors??[])releaseAgentConnection(actor)
        clock = Date.now()
        if(command)void processCommand(command)
        if (!stopped) timer = setTimeout(() => void poll(), 400)
      } catch (cause) {
        if (!stopped) { error = cause instanceof Error ? cause.message : String(cause); connected = false }
      }
    }
    async function heartbeat(): Promise<void> {
      try {
        const response = await request('/api/agent/heartbeat', { editorId, includeConnections:true })
        if (!response.ok) throw new Error(`Связь с редактором потеряна (${response.status})`)
        if(stopped)return
        if(response.status===200){const snapshot=await response.json();if(stopped)return;updateAgentConnections(snapshot.connections);for(const actor of snapshot.releasedActors??[])releaseAgentConnection(actor)}
        const released=response.headers.get('x-agent-released')
        if(released)for(const actor of JSON.parse(released) as string[])releaseAgentConnection(actor)
        if (!stopped) heartbeatTimer = setTimeout(() => void heartbeat(), 1000)
      } catch (cause) { if (!stopped) { error = cause instanceof Error ? cause.message : String(cause); connected = false } }
    }
    void poll()
    heartbeatTimer = setTimeout(() => void heartbeat(), 1000)
    return () => {
      stopped = true
      agentConnections.items=[]
      controller.abort()
      if (timer) clearTimeout(timer)
      if (heartbeatTimer) clearTimeout(heartbeatTimer)
      if(agentSession.work)endAgentWork('user',true)
      agentSession.lock = null
    }
  })
</script>

<details class="composition-tool-section agent-panel" bind:open={expanded}>
  <summary>Агент и история</summary>
  <div class="composition-tool-body">
    {#if agentSession.work}
      <p role="status">Агент работает: {agentSession.work.reason}</p>
      <button class="btn ghost sm" type="button" onclick={()=>act('end_agent_session',{force:true})}>Завершить работу агента</button>
    {/if}
    {#if activeLock}<button class="btn ghost sm" type="button" onclick={()=>{agentSession.lock=null}}>Снять блокировку истории</button>{/if}
    <p>MCP: <code>/mcp</code>. Задайте серверу VIDEO_KADR_MCP_TOKEN (от 32 символов), затем подключите эту композицию тем же ключом. Ключ остаётся в памяти вкладки.</p>
    <label>Ключ подключения <input type="password" bind:value={token} disabled={connected} autocomplete="off" /></label>
    <button class="btn ghost sm" type="button" disabled={!connected && token.trim().length < 32} onclick={() => { error = ''; connected = !connected }}> {connected ? 'Отключить агента' : 'Подключить агента'} </button>
    {#if connected}<p role="status">Редактор подключён к MCP</p>
      <section aria-label="Клиенты MCP"><p>Сессий клиентов: {agentConnections.items.length}</p><ul>{#each agentConnections.items as client (client.id)}<li data-agent-client={client.id}>{client.client}{client.version?` · версия ${client.version}`:''}<details><summary>Сведения о подключении</summary><p>Подключён: {new SvelteDate(client.connectedAt).toLocaleString()}</p><p>Последний запрос: {new SvelteDate(client.lastActivityAt).toLocaleString()}</p></details></li>{/each}</ul></section>
    {/if}
    {#if activeLock}<p role="status">История занята: {activeLock.reason} ({activeLock.owner})</p>{/if}
    <label>Контрольная точка <input bind:value={label} maxlength="128" /></label>
    <button class="btn ghost sm" type="button" disabled={!label.trim()} onclick={() => { act('create_checkpoint', { label }); label = '' }}>Сохранить точку</button>
    {#each checkpoints as checkpoint (checkpoint.id)}
      <div>
        <span>{checkpoint.label}</span>
        <button class="btn ghost sm" type="button" disabled={Boolean(activeLock)} onclick={() => act('restore_checkpoint', { id: checkpoint.id })}>Восстановить</button>
        <button class="btn ghost sm" type="button" onclick={() => act('delete_checkpoint', { id: checkpoint.id })}>Удалить точку</button>
      </div>
    {/each}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <AgentActivityPanel />
    <details><summary>Диагностический журнал</summary>
    <ol aria-label="Журнал действий агента">
      {#each agentSession.log.slice(-20).toReversed() as entry (`${entry.id}-${entry.status}`)}
        <li>{new Date(entry.time).toLocaleTimeString()} · {entry.actor} · {entry.tool} · {entry.status}{entry.detail ? `: ${entry.detail}` : ''}</li>
      {/each}
    </ol>
    </details>
  </div>
</details>
