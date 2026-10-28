export type MoneyMinor = string
export interface AppStatus {
  appVersion: string
  schemaVersion: number
  databaseReady: boolean
  databasePath: string
  bookName: string
  currency: string
}
export interface AppError {
  code: string
  message: string
  traceId: string
}
export type ApiResult<T> =
  { ok: true; data: T } | { ok: false; error: AppError }
