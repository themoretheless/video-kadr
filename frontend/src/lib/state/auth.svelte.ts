import {
  loginAuthUser,
  logoutAuthSession,
  registerAuthUser,
  type AuthSessionDto,
  type AuthUserDto,
} from '$lib/api.js'

export const authState = $state<{
  token: string | null
  user: AuthUserDto | null
  expiresAt: number | null
}>({ token: null, user: null, expiresAt: null })

function accept(session: AuthSessionDto): void {
  authState.token = session.token
  authState.user = session.user
  authState.expiresAt = session.expiresAt
}

export async function login(username: string, password: string): Promise<void> {
  accept(await loginAuthUser(username, password))
}

export async function register(username: string, password: string): Promise<void> {
  accept(await registerAuthUser(username, password))
}

export async function logout(): Promise<void> {
  const token = authState.token
  authState.token = null
  authState.user = null
  authState.expiresAt = null
  if (token) await logoutAuthSession(token)
}

export function setAuthSessionForTests(session: AuthSessionDto | null): void {
  if (session) accept(session)
  else {
    authState.token = null
    authState.user = null
    authState.expiresAt = null
  }
}
