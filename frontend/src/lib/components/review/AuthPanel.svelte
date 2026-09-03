<script lang="ts">
  import { login, register } from '$lib/state/auth.svelte.js'

  let mode = $state<'login' | 'register'>('login')
  let username = $state('')
  let password = $state('')
  let busy = $state(false)
  let error = $state('')

  async function submit(): Promise<void> {
    if (busy) return
    busy = true
    error = ''
    try {
      if (mode === 'register') await register(username.trim(), password)
      else await login(username.trim(), password)
      password = ''
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause)
    } finally {
      busy = false
    }
  }
</script>

<section class="review-auth" aria-label="Вход для совместного ревью">
  <div><h3>{mode === 'login' ? 'Войти в Review' : 'Создать аккаунт'}</h3><p>Автор комментариев определяется защищённой сессией.</p></div>
  <form onsubmit={(event) => { event.preventDefault(); void submit() }}>
    <label for="review-auth-username">Имя пользователя</label>
    <input id="review-auth-username" autocomplete="username" minlength="3" maxlength="64" required bind:value={username} />
    <label for="review-auth-password">Пароль</label>
    <input id="review-auth-password" type="password" autocomplete={mode === 'login' ? 'current-password' : 'new-password'} minlength="12" maxlength="1024" required bind:value={password} />
    <button class="btn primary sm" type="submit" disabled={busy || username.trim().length < 3 || password.length < 12}>{busy ? 'Подождите…' : mode === 'login' ? 'Войти' : 'Создать и войти'}</button>
  </form>
  <button class="btn ghost sm" type="button" disabled={busy} onclick={() => { mode = mode === 'login' ? 'register' : 'login'; error = '' }}>
    {mode === 'login' ? 'Нет аккаунта? Регистрация' : 'Уже есть аккаунт? Войти'}
  </button>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .review-auth { display: grid; gap: 12px; padding: 14px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--panel-2); }
  .review-auth h3, .review-auth p { margin: 0; }
  .review-auth h3 { font-size: .95rem; }
  .review-auth div p { margin-top: 3px; color: var(--muted); font-size: .75rem; }
  .review-auth form { display: grid; gap: 7px; }
  .review-auth label { font-size: .74rem; font-weight: 650; }
  .review-auth .error { color: var(--danger); font-size: .75rem; }
</style>
