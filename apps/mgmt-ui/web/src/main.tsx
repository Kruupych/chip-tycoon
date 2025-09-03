import React from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'

// Dev-only global error logging to help surface IPC issues
if (import.meta && (import.meta as any).env && (import.meta as any).env.DEV) {
  window.addEventListener('error', (e) => {
    console.error('GlobalError:', e.error || e.message)
  })
  window.addEventListener('unhandledrejection', (e) => {
    console.error('UnhandledRejection:', (e.reason as any)?.message || e.reason)
  })
}

const container = document.getElementById('root')
if (container) {
  const root = createRoot(container)
  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  )
}
