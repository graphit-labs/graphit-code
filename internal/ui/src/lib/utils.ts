import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function getApiBase(): string {
  if (window.__API_BASE__) return window.__API_BASE__
  
  return '/api'
}

export function getAppMode(): 'hub' | 'ast' | 'unified' {
  const mode = window.__APP_MODE__
  if (mode === 'ast' || mode === 'unified') return mode
  return 'hub'
}

export function isWebMode(): boolean {
  return !!window.__WEB_MODE__
}

export function getWebUser(): string {
  return window.__WEB_USER__ ?? ''
}

export function bumpPatch(version: string): string {
  const parts = version.split('.')
  if (parts.length === 3) {
    parts[2] = String(parseInt(parts[2] || '0') + 1)
    return parts.join('.')
  }
  return version
}

export function formatCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`
  return String(n)
}

export function truncatePath(p: string, maxLen = 60): string {
  if (p.length <= maxLen) return p
  return '…' + p.slice(-(maxLen - 1))
}

export function debounce<T extends (...args: unknown[]) => void>(fn: T, ms: number): T {
  let timer: ReturnType<typeof setTimeout>
  return ((...args: unknown[]) => {
    clearTimeout(timer)
    timer = setTimeout(() => fn(...args), ms)
  }) as T
}

const colorCache = new Map<string, string>()
const palette = [
  '#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6',
  '#06b6d4', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
  '#84cc16', '#e11d48', '#0ea5e9', '#d97706', '#7c3aed',
]

export function labelColor(label: string): string {
  if (colorCache.has(label)) return colorCache.get(label)!
  let hash = 0
  for (let i = 0; i < label.length; i++) hash = (hash * 31 + label.charCodeAt(i)) >>> 0
  const color = palette[hash % palette.length]
  colorCache.set(label, color)
  return color
}

export function wikiLinkFriendlyName(raw: string): string {
  return raw
    .replace(/\.md$/, '')
    .replace(/^ADR-_/, 'ADR: ')
    .replace(/^ADR:_/, 'ADR: ')
    .replace(/^community-/, '')
    .replace(/^god-node-/, '')
    .replace(/_/g, ' ')
    .replace(/—/g, ' — ')
    .replace(/\s+/g, ' ')
    .trim()
}
