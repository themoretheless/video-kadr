<script lang="ts">
  import {
    createProjectReviewThread,
    createProjectReviewShare,
    getProjectReviewAudit,
    getProjectReviewMembers,
    getProjectReviewThreads,
    replyToProjectReviewThread,
    revokeProjectReviewShare,
    setProjectReviewMember,
    setProjectReviewThreadResolved,
    transferCompositionProjectOwnership,
    type ProjectReviewRole,
    type ProjectReviewMemberDto,
    type ReviewAuditEventDto,
    type ReviewShareCreatedDto,
    type ReviewThreadDto,
  } from '$lib/api.js'
  import AuthPanel from '$lib/components/review/AuthPanel.svelte'
  import {
    compositionState,
    setCompositionPlayhead,
  } from '$lib/state/composition.svelte.js'
  import { COMPOSITION_TIME_BASE } from '$lib/composition/types.js'
  import { authState, logout } from '$lib/state/auth.svelte.js'

  interface Props {
    onthreadschange?: (threads: ReviewThreadDto[]) => void
  }

  let { onthreadschange }: Props = $props()
  let threads = $state<ReviewThreadDto[]>([])
  let members = $state<ProjectReviewMemberDto[]>([])
  let audit = $state<ReviewAuditEventDto[]>([])
  let share = $state<ReviewShareCreatedDto | null>(null)
  let memberActor = $state('')
  let memberRole = $state<ProjectReviewRole>('commenter')
  let commentBody = $state('')
  let replyBody = $state('')
  let replyingTo = $state<string | null>(null)
  let loading = $state(false)
  let busy = $state(false)
  let error = $state('')
  let message = $state('')
  let loadRevision = 0
  const actor = $derived(authState.user?.username ?? '')
  const token = $derived(authState.token ?? '')
  const currentRole = $derived(members.find((member) => member.actor === actor)?.role ?? (members.length === 0 ? 'owner' : 'viewer'))

  $effect(() => {
    const projectId = compositionState.projectId
    threads = []
    members = []
    audit = []
    share = null
    onthreadschange?.([])
    replyingTo = null
    error = ''
    message = ''
    const revision = ++loadRevision
    if (projectId && token) void load(projectId, token, revision)
  })

  async function load(projectId = compositionState.projectId, sessionToken = token, revision = ++loadRevision): Promise<void> {
    if (!projectId || !sessionToken) return
    loading = true
    error = ''
    try {
      const [result, projectMembers, projectAudit] = await Promise.all([
        getProjectReviewThreads(projectId, sessionToken),
        getProjectReviewMembers(projectId, sessionToken),
        getProjectReviewAudit(projectId, sessionToken),
      ])
      if (revision === loadRevision && projectId === compositionState.projectId) {
        publish(result)
        members = projectMembers
        audit = projectAudit
      }
    } catch (cause) {
      if (revision === loadRevision) error = messageFor(cause)
    } finally {
      if (revision === loadRevision) loading = false
    }
  }

  async function createThread(): Promise<void> {
    const projectId = compositionState.projectId
    const body = commentBody.trim()
    if (!projectId || !body || !token || busy) return
    busy = true
    error = ''
    message = ''
    try {
      const thread = await createProjectReviewThread(
        projectId,
        token,
        body,
        Math.round(compositionState.transport.playheadTicks),
      )
      publish([...threads, thread])
      commentBody = ''
      if (members.length === 0) members = [{ actor, role: 'owner' }]
      await refreshAudit(projectId)
      message = 'Комментарий добавлен на текущем таймкоде'
    } catch (cause) {
      error = messageFor(cause)
    } finally {
      busy = false
    }
  }

  async function reply(threadId: string): Promise<void> {
    const body = replyBody.trim()
    if (!body || !token || busy) return
    busy = true
    error = ''
    try {
      replaceThread(await replyToProjectReviewThread(threadId, token, body))
      replyBody = ''
      replyingTo = null
      message = 'Ответ опубликован'
      if (compositionState.projectId) await refreshAudit(compositionState.projectId)
    } catch (cause) {
      error = messageFor(cause)
    } finally {
      busy = false
    }
  }

  async function setResolved(thread: ReviewThreadDto, resolved: boolean): Promise<void> {
    if (busy || !token) return
    busy = true
    error = ''
    try {
      replaceThread(await setProjectReviewThreadResolved(thread.id, token, resolved))
      message = resolved ? 'Ветка закрыта' : 'Ветка снова открыта'
      if (compositionState.projectId) await refreshAudit(compositionState.projectId)
    } catch (cause) {
      error = messageFor(cause)
    } finally {
      busy = false
    }
  }

  function replaceThread(updated: ReviewThreadDto): void {
    publish(threads.map((thread) => thread.id === updated.id ? updated : thread))
  }

  function publish(updated: ReviewThreadDto[]): void {
    threads = updated
    onthreadschange?.(updated)
  }

  function seek(thread: ReviewThreadDto): void {
    const tick = thread.comments[0]?.timelineTick ?? 0
    setCompositionPlayhead(tick)
    message = `Плейхед: ${formatTick(tick)}`
  }

  function startReply(threadId: string): void {
    replyingTo = replyingTo === threadId ? null : threadId
    replyBody = ''
  }

  async function saveMember(): Promise<void> {
    const projectId = compositionState.projectId
    const targetActor = memberActor.trim()
    if (!projectId || !targetActor || !token || busy || currentRole !== 'owner') return
    busy = true
    error = ''
    try {
      const updated = await setProjectReviewMember(projectId, token, targetActor, memberRole)
      members = [...members.filter((member) => member.actor !== updated.actor), updated]
        .sort((left, right) => left.actor.localeCompare(right.actor))
      memberActor = ''
      message = `Роль ${updated.actor}: ${updated.role}`
      await refreshAudit(projectId)
    } catch (cause) {
      error = messageFor(cause)
    } finally {
      busy = false
    }
  }

  async function transferOwnership(targetActor: string): Promise<void> {
    const projectId = compositionState.projectId
    if (!projectId || !token || busy || currentRole !== 'owner' || targetActor === actor) return
    busy = true
    error = ''
    try {
      await transferCompositionProjectOwnership(projectId, token, targetActor)
      members = members.map((member) => member.actor === targetActor
        ? { ...member, role: 'owner' }
        : member.actor === actor ? { ...member, role: 'editor' } : member)
      message = `Проект передан пользователю ${targetActor}`
      await refreshAudit(projectId)
    } catch (cause) {
      error = messageFor(cause)
    } finally {
      busy = false
    }
  }

  function formatTick(tick: number): string {
    const totalSeconds = Math.max(0, tick) / COMPOSITION_TIME_BASE
    const minutes = Math.floor(totalSeconds / 60)
    const seconds = totalSeconds - minutes * 60
    return `${minutes}:${seconds.toFixed(3).padStart(6, '0')}`
  }

  async function refreshAudit(projectId: string): Promise<void> {
    try {
      if (!token) return
      const updated = await getProjectReviewAudit(projectId, token)
      if (projectId === compositionState.projectId) audit = updated
    } catch {
      // The primary mutation already succeeded; refresh remains best-effort.
    }
  }

  async function createShare(): Promise<void> {
    const projectId = compositionState.projectId
    if (!projectId || !token || busy || currentRole !== 'owner') return
    busy = true
    error = ''
    try {
      share = await createProjectReviewShare(projectId, token)
      await refreshAudit(projectId)
      message = 'Review-ссылка создана на 7 дней'
    } catch (cause) { error = messageFor(cause) } finally { busy = false }
  }

  async function revokeShare(): Promise<void> {
    const projectId = compositionState.projectId
    if (!projectId || !share || !token || busy) return
    busy = true
    error = ''
    try {
      await revokeProjectReviewShare(projectId, share.grant.id, token)
      share = null
      await refreshAudit(projectId)
      message = 'Review-ссылка отозвана'
    } catch (cause) { error = messageFor(cause) } finally { busy = false }
  }

  async function signOut(): Promise<void> {
    if (busy) return
    busy = true
    try {
      await logout()
    } catch {
      // logout clears the local in-memory session before best-effort revoke.
    } finally {
      busy = false
    }
  }

  function shareUrl(): string {
    return share ? `${location.origin}/review/${encodeURIComponent(share.token)}` : ''
  }

  function messageFor(cause: unknown): string {
    return cause instanceof Error ? cause.message : String(cause)
  }
</script>

<section class="composition-review card" aria-label="Комментарии ревью">
  <header>
    <div>
      <h2>Review</h2>
      <p>Комментарии привязаны к таймкоду монтажной линии.</p>
    </div>
    {#if compositionState.projectId && authState.user}
      <div class="composition-review-session">
        <span>{authState.user.username}</span>
        <button class="btn ghost sm" type="button" disabled={loading || busy} onclick={() => void load()}>Обновить</button>
        <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void signOut()}>Выйти</button>
      </div>
    {/if}
  </header>

  {#if !compositionState.projectId}
    <p class="composition-review-empty" role="status">Сохраните композицию, чтобы обсуждать монтаж.</p>
  {:else if !authState.user || !authState.token}
    <AuthPanel />
  {:else}
    <form onsubmit={(event) => { event.preventDefault(); void createThread() }}>
      <label for="composition-review-comment">Комментарий на {formatTick(compositionState.transport.playheadTicks)}</label>
      <textarea id="composition-review-comment" maxlength="8192" rows="2" bind:value={commentBody} placeholder="Что нужно изменить?"></textarea>
      <button class="btn primary sm" type="submit" disabled={busy || !commentBody.trim()}>Добавить комментарий</button>
    </form>

    {#if loading}<p role="status">Загружаю комментарии…</p>{/if}
    {#if !loading && threads.length === 0}<p class="composition-review-empty">Комментариев пока нет.</p>{/if}

    <div class="composition-review-list">
      {#each threads as thread (thread.id)}
        <article class:resolved={thread.resolvedAt != null} aria-label={`Ветка на ${formatTick(thread.comments[0]?.timelineTick ?? 0)}`}>
          <div class="composition-review-threadbar">
            <button class="composition-review-time" type="button" onclick={() => seek(thread)}>
              {formatTick(thread.comments[0]?.timelineTick ?? 0)}
            </button>
            <span>{thread.resolvedAt == null ? 'Открыто' : `Закрыто · ${thread.resolvedBy ?? ''}`}</span>
          </div>
          <ol>
            {#each thread.comments as comment (comment.id)}
              <li><strong>{comment.author}</strong><p>{comment.body}</p></li>
            {/each}
          </ol>
          <div class="composition-review-actions">
            {#if thread.resolvedAt == null}
              <button class="btn ghost sm" type="button" disabled={busy || currentRole === 'viewer'} onclick={() => startReply(thread.id)}>Ответить</button>
              <button class="btn ghost sm" type="button" disabled={busy || (currentRole !== 'owner' && currentRole !== 'editor')} onclick={() => void setResolved(thread, true)}>Закрыть</button>
            {:else}
              <button class="btn ghost sm" type="button" disabled={busy || (currentRole !== 'owner' && currentRole !== 'editor')} onclick={() => void setResolved(thread, false)}>Открыть снова</button>
            {/if}
          </div>
          {#if replyingTo === thread.id}
            <form class="composition-review-reply" onsubmit={(event) => { event.preventDefault(); void reply(thread.id) }}>
              <label for={`composition-review-reply-${thread.id}`}>Ответ</label>
              <textarea id={`composition-review-reply-${thread.id}`} maxlength="8192" rows="2" bind:value={replyBody}></textarea>
              <button class="btn primary sm" type="submit" disabled={busy || !replyBody.trim()}>Опубликовать</button>
            </form>
          {/if}
        </article>
      {/each}
    </div>

    <details class="composition-review-members">
      <summary>Участники · {members.length || 1}</summary>
      <ul>
        {#each members as member (member.actor)}
          <li>
            <span>{member.actor}</span><strong>{member.role}</strong>
            {#if currentRole === 'owner' && member.actor !== actor}
              <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void transferOwnership(member.actor)}>Передать проект</button>
            {/if}
          </li>
        {/each}
      </ul>
      {#if currentRole === 'owner'}
        <form onsubmit={(event) => { event.preventDefault(); void saveMember() }}>
          <label for="composition-review-member">Участник</label>
          <input id="composition-review-member" maxlength="64" pattern="[A-Za-z0-9._:-]+" bind:value={memberActor} placeholder="editor-name" />
          <select aria-label="Роль участника" bind:value={memberRole}>
            <option value="editor">editor</option><option value="commenter">commenter</option><option value="viewer">viewer</option>
          </select>
          <button class="btn ghost sm" type="submit" disabled={busy || !memberActor.trim()}>Сохранить роль</button>
        </form>
      {/if}
    </details>
    <details class="composition-review-audit">
      <summary>История действий · {audit.length}</summary>
      <ol>{#each audit as event (event.id)}<li><strong>{event.actor}</strong> · {event.action}</li>{/each}</ol>
    </details>
    {#if currentRole === 'owner'}
      <div class="composition-review-share">
        {#if share}
          <label for="composition-review-share-url">Ссылка для просмотра · 7 дней</label>
          <input id="composition-review-share-url" readonly value={shareUrl()} />
          <button class="btn ghost sm danger" type="button" disabled={busy} onclick={() => void revokeShare()}>Отозвать ссылку</button>
        {:else}
          <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void createShare()}>Создать review-ссылку</button>
        {/if}
      </div>
    {/if}
  {/if}

  {#if message}<p class="composition-review-message" role="status">{message}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .composition-review { display: grid; gap: 12px; }
  .composition-review header, .composition-review-threadbar, .composition-review-actions { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .composition-review-session { display: flex; align-items: center; gap: 6px; }
  .composition-review-session > span { color: var(--muted); font-size: .72rem; }
  .composition-review h2 { margin: 0; font-size: 1rem; }
  .composition-review header p, .composition-review-empty, .composition-review-message { margin: 3px 0 0; color: var(--muted); font-size: .78rem; }
  .composition-review form { display: grid; gap: 7px; }
  .composition-review label { font-size: .76rem; font-weight: 650; }
  .composition-review textarea { width: 100%; resize: vertical; }
  .composition-review-list { display: grid; gap: 10px; }
  .composition-review article { border: 1px solid var(--line); border-radius: 10px; padding: 10px; background: var(--surface-2, rgba(255,255,255,.03)); }
  .composition-review article.resolved { opacity: .72; }
  .composition-review-time { border: 0; padding: 0; background: transparent; color: var(--accent); font: inherit; font-weight: 700; cursor: pointer; }
  .composition-review-threadbar span { color: var(--muted); font-size: .72rem; }
  .composition-review ol { margin: 9px 0; padding-left: 20px; }
  .composition-review li + li { margin-top: 8px; }
  .composition-review li strong { font-size: .74rem; }
  .composition-review li p { margin: 2px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; }
  .composition-review-actions { justify-content: flex-start; }
  .composition-review-reply { margin-top: 9px; }
  .composition-review-members { border-top: 1px solid var(--line); padding-top: 8px; }
  .composition-review-members summary { cursor: pointer; font-size: .78rem; font-weight: 700; }
  .composition-review-members ul { display: grid; gap: 4px; margin: 8px 0; padding: 0; list-style: none; }
  .composition-review-members li { display: flex; justify-content: space-between; gap: 8px; font-size: .74rem; }
  .composition-review-members form { grid-template-columns: minmax(120px, 1fr) auto auto; align-items: end; }
  .composition-review-members form label { grid-column: 1 / -1; }
  .composition-review-audit ol { margin: 8px 0 0; padding-left: 20px; font-size: .72rem; color: var(--muted); }
  .composition-review-share { display: grid; gap: 6px; }
</style>
    setProjectReviewMember,
