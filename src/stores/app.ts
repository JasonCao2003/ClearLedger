import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getAppStatus } from '../api/client'
import type { AppError, AppStatus } from '../api/types'

export const useAppStore = defineStore('app', () => {
  const status = ref<AppStatus | null>(null)
  const error = ref<AppError | null>(null)
  const loading = ref(false)
  const theme = ref<'light' | 'dark'>('light')
  const sidebarOpen = ref(false)
  async function refreshStatus() {
    if (loading.value) return
    loading.value = true
    try {
      const result = await getAppStatus()
      status.value = result.ok ? result.data : null
      error.value = result.ok ? null : result.error
    } finally {
      loading.value = false
    }
  }
  function toggleTheme() {
    theme.value = theme.value === 'light' ? 'dark' : 'light'
  }
  return {
    status,
    error,
    loading,
    theme,
    sidebarOpen,
    refreshStatus,
    toggleTheme,
  }
})
