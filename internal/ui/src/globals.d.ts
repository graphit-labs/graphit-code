
declare global {
  interface Window {
    __API_BASE__?: string
    __APP_MODE__?: 'hub' | 'ast' | 'unified'
    __WEB_MODE__?: boolean
    __WEB_USER__?: string
    __LOGIN_URL__?: string
    __LOGOUT_URL__?: string
    __PROJECT_NAME__?: string
  }
}

export {}
