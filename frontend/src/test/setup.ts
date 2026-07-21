import { Window } from 'happy-dom'

// Recent Node versions expose an unconfigured global localStorage accessor.
// Use an explicit happy-dom Window so that accessor cannot shadow test storage.
const storageWindow = new Window({ url: 'http://localhost' })
Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  value: storageWindow.localStorage,
})
