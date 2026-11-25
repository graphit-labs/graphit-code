import { api } from './client'

export interface DaemonStatus {
  pid: number
  state: 'running' | 'stopping'
}

export const daemonApi = {
  getStatus: async (signal?: AbortSignal): Promise<DaemonStatus> => {
    const value = await api.get<unknown>('/daemon/status', { signal, cache: 'no-store' })
    if (!value || typeof value !== 'object' || !('pid' in value) || !('state' in value)
      || typeof value.pid !== 'number' || !Number.isInteger(value.pid) || value.pid < 0 || value.pid > 0xffffffff
      || (value.state !== 'running' && value.state !== 'stopping')) {
      throw new Error('Invalid daemon status response.')
    }
    return { pid: value.pid, state: value.state }
  },
}
