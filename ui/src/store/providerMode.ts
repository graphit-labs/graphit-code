import { useAppStore } from './appStore'

export function useProviderMode() {
  return useAppStore(state => {
    const identity = state.webIdentity
    if (identity.state === 'loading' || identity.state === 'unavailable') return identity.state
    return identity.state === 'anonymous' && identity.local ? 'local' : 'broker'
  })
}
