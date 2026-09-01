
declare global {
  interface Window {
    __API_BASE__?: string
    __APP_MODE__?: 'hub' | 'ast' | 'unified'
    __WEB_MODE__?: boolean
    __WEB_USER__?: string
    __LOGIN_URL__?: string
    __LOGOUT_URL__?: string
    __PROJECT_NAME__?: string
    // Injected by the server. Undefined means "assume available", which is right for the
    // hub-only server and for `npm run dev`, where no Go server injected anything.
    __AGENT_FEATURES__?: boolean
  }
}

export {}
