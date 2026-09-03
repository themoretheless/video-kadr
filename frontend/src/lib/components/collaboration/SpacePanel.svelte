<script lang="ts">
  import { SvelteURLSearchParams } from 'svelte/reactivity'
  import { acceptSpaceInvite, compositionProjectArchiveUrl, createSpace, createSpaceInvite, deleteCompositionProject, deleteLibraryItem, deleteSpace, getCompositionProjects, getLibrary, getSpaceMembers, getSpaces, removeSpaceMember, renameSpace, setSpaceMember, transferSpaceOwnership, type SpaceDto, type SpaceMemberDto, type SpaceRole } from '$lib/api.js'
  import { authState } from '$lib/state/auth.svelte.js'
  import { spacesState } from '$lib/state/spaces.svelte.js'

  let { onselectionchange = () => {} } = $props<{ onselectionchange?: () => void }>()

  let spaces = $state<SpaceDto[]>([])
  let members = $state<SpaceMemberDto[]>([])
  let newName = $state('')
  let renameName = $state('')
  let memberActor = $state('')
  let memberRole = $state<Exclude<SpaceRole, 'owner'>>('editor')
  let inviteRole = $state<Exclude<SpaceRole, 'owner'>>('editor')
  let inviteLink = $state('')
  let acceptingInvite = ''
  let transferActor = $state('')
  let busy = $state(false)
  let error = $state('')
  let exportMessage = $state('')
  const selected = $derived(spaces.find((space) => space.id === spacesState.selectedId) ?? null)

  $effect(() => {
    const token = authState.token
    if (token) { void load(token); void acceptInviteFromUrl(token) }
    else { spaces = []; spacesState.selectedId = ''; members = [] }
  })

  async function load(token: string): Promise<void> {
    try {
      spaces = await getSpaces(token)
      if (!spaces.some((space) => space.id === spacesState.selectedId)) {
        spacesState.selectedId = spaces[0]?.id ?? ''
        onselectionchange()
      }
      renameName = spaces.find((space) => space.id === spacesState.selectedId)?.name ?? ''
      if (spacesState.selectedId) members = await getSpaceMembers(spacesState.selectedId, token)
      transferActor = members.find((member) => member.role !== 'owner')?.actor ?? ''
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }

  async function selectSpace(event: Event): Promise<void> {
    const selectedId = (event.currentTarget as HTMLSelectElement).value
    spacesState.selectedId = selectedId
    onselectionchange()
    const token = authState.token
    if (!token || !spacesState.selectedId) { members = []; return }
    renameName = spaces.find((space) => space.id === selectedId)?.name ?? ''
    try {
      members = await getSpaceMembers(spacesState.selectedId, token)
      transferActor = members.find((member) => member.role !== 'owner')?.actor ?? ''
    }
    catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }

  async function renameSelected(): Promise<void> {
    const token = authState.token
    const current = selected
    const name = renameName.trim()
    if (!token || !current || current.role !== 'owner' || !name || name === current.name || busy) return
    busy = true; error = ''
    try {
      const updated = await renameSpace(current.id, token, name, current.updatedAt)
      spaces = spaces.map((space) => space.id === updated.id ? updated : space)
      renameName = updated.name
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function deleteSelected(): Promise<void> {
    const token = authState.token
    const current = selected
    if (!token || !current || current.role !== 'owner' || busy) return
    if (!window.confirm(`Удалить пустое пространство «${current.name}»?`)) return
    busy = true; error = ''
    try {
      await deleteSpace(current.id, token)
      spaces = spaces.filter((space) => space.id !== current.id)
      spacesState.selectedId = spaces[0]?.id ?? ''
      renameName = spaces[0]?.name ?? ''
      members = spacesState.selectedId ? await getSpaceMembers(spacesState.selectedId, token) : []
      onselectionchange()
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function transferSelected(): Promise<void> {
    const token = authState.token
    const current = selected
    if (!token || !current || current.role !== 'owner' || !transferActor || busy) return
    if (!window.confirm(`Передать пространство «${current.name}» пользователю ${transferActor}?`)) return
    busy = true; error = ''
    try {
      await transferSpaceOwnership(current.id, token, transferActor)
      const [nextSpaces, nextMembers] = await Promise.all([getSpaces(token), getSpaceMembers(current.id, token)])
      spaces = nextSpaces
      members = nextMembers
      renameName = spaces.find((space) => space.id === current.id)?.name ?? ''
      transferActor = ''
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function exportProjects(): Promise<void> {
    const token = authState.token
    const current = selected
    if (!token || !current || busy) return
    busy = true; error = ''; exportMessage = ''
    try {
      const projects = (await getCompositionProjects(token)).filter((project) => project.spaceId === current.id)
      if (!projects.length) { exportMessage = 'В пространстве пока нет проектов'; return }
      for (const project of projects) {
        const anchor = document.createElement('a')
        anchor.href = compositionProjectArchiveUrl(project.id)
        anchor.download = ''
        anchor.rel = 'noopener'
        anchor.click()
      }
      exportMessage = `Начат экспорт ${projects.length} .veproj; браузер может запросить разрешение на несколько загрузок`
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function teardownSelected(): Promise<void> {
    const token = authState.token
    const current = selected
    if (!token || !current || current.role !== 'owner' || busy) return
    const confirmation = window.prompt(`Введите «${current.name}», чтобы удалить Space, все проекты и медиа`)
    if (confirmation !== current.name) return
    busy = true; error = ''; exportMessage = 'Начато удаление содержимого Space'
    try {
      const projects = (await getCompositionProjects(token)).filter((project) => project.spaceId === current.id)
      for (const project of projects) await deleteCompositionProject(project.id, token)
      const media = await getLibrary(token, current.id)
      for (const entry of media) await deleteLibraryItem(entry.id, token, current.id)
      await deleteSpace(current.id, token)
      spaces = spaces.filter((space) => space.id !== current.id)
      spacesState.selectedId = spaces[0]?.id ?? ''
      renameName = spaces[0]?.name ?? ''
      members = spacesState.selectedId ? await getSpaceMembers(spacesState.selectedId, token) : []
      exportMessage = `Space «${current.name}» и его содержимое удалены`
      onselectionchange()
    } catch (cause) {
      error = `${cause instanceof Error ? cause.message : String(cause)}. Уже выполненные шаги сохранены; повторите удаление.`
      exportMessage = ''
    } finally { busy = false }
  }

  async function addSpace(): Promise<void> {
    const token = authState.token
    if (!token || !newName.trim() || busy) return
    busy = true; error = ''
    try {
      const space = await createSpace(token, newName.trim())
      spaces = [space, ...spaces]
      spacesState.selectedId = space.id
      renameName = space.name
      onselectionchange()
      members = [{ actor: authState.user?.username ?? '', role: 'owner' }]
      newName = ''
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function addMember(): Promise<void> {
    const token = authState.token
    const actor = memberActor.trim()
    if (!token || !spacesState.selectedId || !actor || busy || selected?.role !== 'owner') return
    busy = true; error = ''
    try {
      const member = await setSpaceMember(spacesState.selectedId, token, actor, memberRole)
      members = [...members.filter((item) => item.actor !== actor), member].sort((a, b) => a.actor.localeCompare(b.actor))
      memberActor = ''
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function createInvite(): Promise<void> {
    const token = authState.token
    const spaceId = spacesState.selectedId
    if (!token || !spaceId || busy || selected?.role !== 'owner') return
    busy = true; error = ''; inviteLink = ''
    try {
      const invite = await createSpaceInvite(spaceId, token, inviteRole)
      const params = new SvelteURLSearchParams(window.location.search)
      params.set('spaceInvite', invite.token)
      inviteLink = `${window.location.origin}${window.location.pathname}?${params}${window.location.hash}`
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }

  async function acceptInviteFromUrl(token: string): Promise<void> {
    const params = new SvelteURLSearchParams(window.location.search)
    const inviteToken = params.get('spaceInvite') ?? ''
    if (!inviteToken || acceptingInvite === inviteToken) return
    acceptingInvite = inviteToken
    try {
      const space = await acceptSpaceInvite(inviteToken, token)
      spacesState.selectedId = space.id
      await load(token)
      onselectionchange()
      params.delete('spaceInvite')
      window.history.replaceState(null, '', `${window.location.pathname}${params.size ? `?${params}` : ''}${window.location.hash}`)
      exportMessage = `Invite принят: ${space.name} · ${space.role}`
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }

  async function removeMember(actor: string): Promise<void> {
    const token = authState.token
    if (!token || !spacesState.selectedId || busy || selected?.role !== 'owner') return
    busy = true; error = ''
    try {
      await removeSpaceMember(spacesState.selectedId, token, actor)
      members = members.filter((member) => member.actor !== actor)
      if (transferActor === actor) transferActor = members.find((member) => member.role !== 'owner')?.actor ?? ''
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    finally { busy = false }
  }
</script>

<details class="space-panel card">
  <summary>Рабочие пространства · {spaces.length}</summary>
  <form onsubmit={(event) => { event.preventDefault(); void addSpace() }}>
    <label for="new-space-name">Новое пространство</label>
    <input id="new-space-name" maxlength="128" bind:value={newName} placeholder="Команда запуска" />
    <button class="btn primary sm" disabled={busy || !newName.trim()}>Создать</button>
  </form>
  {#if spaces.length}
    <select aria-label="Рабочее пространство" value={spacesState.selectedId} onchange={(event) => void selectSpace(event)}>
      {#each spaces as space (space.id)}<option value={space.id}>{space.name} · {space.role}</option>{/each}
    </select>
    <button class="btn ghost sm" disabled={busy} onclick={() => void exportProjects()}>Экспорт всех .veproj</button>
    {#if selected?.role === 'owner'}
      <form onsubmit={(event) => { event.preventDefault(); void renameSelected() }}>
        <label for="rename-space-name">Название пространства</label>
        <input id="rename-space-name" maxlength="128" bind:value={renameName} placeholder={selected.name} />
        <button class="btn ghost sm" disabled={busy || !renameName.trim() || renameName.trim() === selected.name}>Переименовать</button>
        <button type="button" class="btn danger sm" disabled={busy} onclick={() => void deleteSelected()}>Удалить пустое</button>
        <button type="button" class="btn danger sm" disabled={busy} onclick={() => void teardownSelected()}>Удалить со всем содержимым</button>
      </form>
    {/if}
    <ul>{#each members as member (member.actor)}<li><span>{member.actor}</span><strong>{member.role}</strong>{#if selected?.role === 'owner' && member.role !== 'owner'}<button class="btn ghost sm" aria-label={`Удалить ${member.actor} из пространства`} disabled={busy} onclick={() => void removeMember(member.actor)}>Удалить</button>{/if}</li>{/each}</ul>
    {#if selected?.role === 'owner'}
      <form onsubmit={(event) => { event.preventDefault(); void addMember() }}>
        <label for="space-member-name">Добавить участника</label>
        <input id="space-member-name" maxlength="64" pattern="[A-Za-z0-9._:-]+" bind:value={memberActor} />
        <select aria-label="Роль в пространстве" bind:value={memberRole}><option value="editor">editor</option><option value="viewer">viewer</option></select>
        <button class="btn ghost sm" disabled={busy || !memberActor.trim()}>Добавить</button>
      </form>
      <form onsubmit={(event) => { event.preventDefault(); void createInvite() }}>
        <label for="space-invite-role">Invite-ссылка на 7 дней</label>
        <select id="space-invite-role" bind:value={inviteRole}><option value="editor">editor</option><option value="viewer">viewer</option></select>
        <button class="btn ghost sm" disabled={busy}>Создать invite</button>
      </form>
      {#if inviteLink}<label for="space-invite-link">Одноразовая ссылка<input id="space-invite-link" readonly value={inviteLink} onclick={(event) => event.currentTarget.select()} /></label>{/if}
      {#if members.some((member) => member.role !== 'owner')}
        <form onsubmit={(event) => { event.preventDefault(); void transferSelected() }}>
          <label for="space-transfer-owner">Передать владение</label>
          <select id="space-transfer-owner" bind:value={transferActor}>{#each members.filter((member) => member.role !== 'owner') as member (member.actor)}<option value={member.actor}>{member.actor}</option>{/each}</select>
          <button class="btn danger sm" disabled={busy || !transferActor}>Передать</button>
        </form>
      {/if}
    {/if}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if exportMessage}<p role="status">{exportMessage}</p>{/if}
</details>

<style>
  .space-panel { display: grid; gap: 10px; padding: 12px; }
  summary { cursor: pointer; font-weight: 700; }
  form { display: flex; gap: 8px; align-items: end; flex-wrap: wrap; }
  label { width: 100%; font-size: .75rem; font-weight: 650; }
  input { min-width: 180px; flex: 1; }
  ul { display: grid; gap: 5px; margin: 0; padding: 0; list-style: none; }
  li { display: flex; justify-content: space-between; gap: 12px; font-size: .78rem; }
  .error { color: var(--danger); }
</style>
