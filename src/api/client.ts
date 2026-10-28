import { invoke, isTauri } from '@tauri-apps/api/core'
import type { ApiResult, AppStatus } from './types'

export const desktopAvailable = isTauri()

export async function getAppStatus(): Promise<ApiResult<AppStatus>> {
  if (!desktopAvailable) {
    return {
      ok: false,
      error: {
        code: 'DESKTOP_REQUIRED',
        message: '当前为浏览器界面预览，请启动桌面应用连接本地数据库。',
        traceId: 'browser-preview',
      },
    }
  }
  try {
    return await invoke<ApiResult<AppStatus>>('get_app_status')
  } catch {
    return {
      ok: false,
      error: {
        code: 'IPC_UNAVAILABLE',
        message: '无法连接本地服务，请重试或重新启动应用。',
        traceId: crypto.randomUUID(),
      },
    }
  }
}
