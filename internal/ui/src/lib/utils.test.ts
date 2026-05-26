import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import {
  cn,
  getApiBase,
  getAppMode,
  isWebMode,
  getWebUser,
  bumpPatch,
  formatCount,
  truncatePath,
  debounce,
  labelColor,
} from './utils'

describe('UI Utilities', () => {
  let originalApiBase: string | undefined
  let originalAppMode: 'hub' | 'ast' | 'unified' | undefined
  let originalWebMode: boolean | undefined
  let originalWebUser: string | undefined

  beforeEach(() => {
    // Save original values
    originalApiBase = window.__API_BASE__
    originalAppMode = window.__APP_MODE__
    originalWebMode = window.__WEB_MODE__
    originalWebUser = window.__WEB_USER__

    // Clean them for a clean slate
    delete window.__API_BASE__
    delete window.__APP_MODE__
    delete window.__WEB_MODE__
    delete window.__WEB_USER__
  })

  afterEach(() => {
    // Restore
    window.__API_BASE__ = originalApiBase
    window.__APP_MODE__ = originalAppMode
    window.__WEB_MODE__ = originalWebMode
    window.__WEB_USER__ = originalWebUser
  })

  describe('cn', () => {
    it('should merge class names correctly', () => {
      expect(cn('class1', 'class2')).toBe('class1 class2')
      expect(cn('class1', { class2: true, class3: false })).toBe('class1 class2')
      expect(cn('class1', null, undefined, 'class2')).toBe('class1 class2')
    })
  })

  describe('getApiBase', () => {
    it('should return window.__API_BASE__ when defined', () => {
      window.__API_BASE__ = '/custom-api'
      expect(getApiBase()).toBe('/custom-api')
    })

    it('should return default /api when window.__API_BASE__ is undefined', () => {
      expect(getApiBase()).toBe('/api')
    })
  })

  describe('getAppMode', () => {
    it('should return window.__APP_MODE__ when it is ast or unified', () => {
      window.__APP_MODE__ = 'ast'
      expect(getAppMode()).toBe('ast')

      window.__APP_MODE__ = 'unified'
      expect(getAppMode()).toBe('unified')
    })

    it('should default to hub when window.__APP_MODE__ is undefined or invalid', () => {
      expect(getAppMode()).toBe('hub')

      window.__APP_MODE__ = 'invalid' as any
      expect(getAppMode()).toBe('hub')
    })
  })

  describe('isWebMode', () => {
    it('should return true when window.__WEB_MODE__ is true', () => {
      window.__WEB_MODE__ = true
      expect(isWebMode()).toBe(true)
    })

    it('should return false when window.__WEB_MODE__ is undefined or false', () => {
      expect(isWebMode()).toBe(false)

      window.__WEB_MODE__ = false
      expect(isWebMode()).toBe(false)
    })
  })

  describe('getWebUser', () => {
    it('should return window.__WEB_USER__ when defined', () => {
      window.__WEB_USER__ = 'user123'
      expect(getWebUser()).toBe('user123')
    })

    it('should return empty string when window.__WEB_USER__ is undefined', () => {
      expect(getWebUser()).toBe('')
    })
  })

  describe('bumpPatch', () => {
    it('should increment patch version on valid semver string', () => {
      expect(bumpPatch('1.2.3')).toBe('1.2.4')
      expect(bumpPatch('0.0.0')).toBe('0.0.1')
      expect(bumpPatch('1.2.')).toBe('1.2.1')
    })

    it('should return version untouched on invalid version strings', () => {
      expect(bumpPatch('1.2')).toBe('1.2')
      expect(bumpPatch('v1.2.3.4')).toBe('v1.2.3.4')
      expect(bumpPatch('')).toBe('')
    })
  })

  describe('formatCount', () => {
    it('should format numbers correctly', () => {
      expect(formatCount(500)).toBe('500')
      expect(formatCount(1000)).toBe('1.0k')
      expect(formatCount(1500)).toBe('1.5k')
      expect(formatCount(9999)).toBe('10.0k')
      expect(formatCount(1000000)).toBe('1.0M')
      expect(formatCount(2500000)).toBe('2.5M')
    })
  })

  describe('truncatePath', () => {
    it('should truncate long paths', () => {
      expect(truncatePath('short/path.txt', 20)).toBe('short/path.txt')
      expect(truncatePath('very/long/nested/path/to/some/file/with/a/long/name.txt', 20)).toBe('…ith/a/long/name.txt')
    })
  })

  describe('debounce', () => {
    it('should debounce calls', () => {
      vi.useFakeTimers()
      const mockFn = vi.fn()
      const debouncedFn = debounce(mockFn, 100)

      debouncedFn('a')
      debouncedFn('b')
      debouncedFn('c')

      expect(mockFn).not.toHaveBeenCalled()

      vi.advanceTimersByTime(50)
      expect(mockFn).not.toHaveBeenCalled()

      vi.advanceTimersByTime(50)
      expect(mockFn).toHaveBeenCalledOnce()
      expect(mockFn).toHaveBeenCalledWith('c')

      vi.useRealTimers()
    })
  })

  describe('labelColor', () => {
    it('should return color from palette and cache the result', () => {
      const color1 = labelColor('label-A')
      expect(color1).toMatch(/^#[0-9a-f]{6}$/i)

      // Calling again should return cached color
      const color2 = labelColor('label-A')
      expect(color2).toBe(color1)

      const color3 = labelColor('label-B')
      expect(color3).toMatch(/^#[0-9a-f]{6}$/i)
    })
  })
})
