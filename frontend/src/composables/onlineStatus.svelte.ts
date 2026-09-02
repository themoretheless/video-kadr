import { listenMany } from './globalListeners.js'

export function createOnlineStatus(source: Pick<Navigator, 'onLine'> = navigator) {
  let online = $state(source.onLine)
  const refresh = () => { online = source.onLine }
  const dispose = listenMany([
    [window, 'online', refresh],
    [window, 'offline', refresh],
  ])
  return {
    get current(): boolean { return online },
    dispose,
  }
}
