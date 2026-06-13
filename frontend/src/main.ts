import { createApp } from 'vue'
import App from './App.vue'
import { state } from './store'
import './style.css'

createApp(App).mount('#app')

// Dev-only debug hook so the editor view can be exercised without a backend.
if (import.meta.env.DEV) {
  ;(window as unknown as Record<string, unknown>).__store = state
}
