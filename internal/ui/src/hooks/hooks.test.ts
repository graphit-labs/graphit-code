import { describe, test, expect, vi, beforeEach } from 'vitest'
import { renderHook, act } from '@testing-library/react'
import { useTheme } from './useTheme'
import { useToast, showToast } from './useToast'

vi.stubGlobal('matchMedia', vi.fn().mockImplementation((query) => ({
  matches: false,
  media: query,
  onchange: null,
  addListener: vi.fn(),
  removeListener: vi.fn(),
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
  dispatchEvent: vi.fn(),
})))

describe('useTheme hook', () => {
  beforeEach(() => {
    localStorage.clear()
    document.documentElement.className = ''
  })

  test('should initialize with light if no storage and prefers-color-scheme is light', () => {
    const { result } = renderHook(() => useTheme())
    expect(result.current.theme).toBe('light')
    expect(document.documentElement.classList.contains('light')).toBe(true)
  })

  test('should initialize with dark if prefers-color-scheme is dark', () => {
    vi.mocked(window.matchMedia).mockImplementationOnce(() => ({
      matches: true,
      media: '',
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }))

    const { result } = renderHook(() => useTheme())
    expect(result.current.theme).toBe('dark')
    expect(document.documentElement.classList.contains('dark')).toBe(true)
  })

  test('should initialize with value from localStorage', () => {
    localStorage.setItem('graphit-theme', 'dark')
    const { result } = renderHook(() => useTheme())
    expect(result.current.theme).toBe('dark')
    expect(document.documentElement.classList.contains('dark')).toBe(true)
  })

  test('should toggle theme', () => {
    const { result } = renderHook(() => useTheme())
    expect(result.current.theme).toBe('light')

    act(() => {
      result.current.toggle()
    })

    expect(result.current.theme).toBe('dark')
    expect(document.documentElement.classList.contains('dark')).toBe(true)
    expect(localStorage.getItem('graphit-theme')).toBe('dark')

    act(() => {
      result.current.toggle()
    })

    expect(result.current.theme).toBe('light')
    expect(document.documentElement.classList.contains('light')).toBe(true)
    expect(localStorage.getItem('graphit-theme')).toBe('light')
  })
})

describe('useToast hook', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  test('should add and remove toasts manually', () => {
    const { result } = renderHook(() => useToast())

    expect(result.current.toasts).toEqual([])

    act(() => {
      result.current.addToast({ id: '1', message: 'Hello', type: 'info' })
    })

    expect(result.current.toasts).toEqual([{ id: '1', message: 'Hello', type: 'info' }])

    act(() => {
      vi.advanceTimersByTime(4000)
    })
    expect(result.current.toasts).toEqual([])
  })

  test('should remove toast by id using removeToast', () => {
    const { result } = renderHook(() => useToast())

    act(() => {
      result.current.addToast({ id: '2', message: 'World', type: 'success' })
    })
    expect(result.current.toasts.length).toBe(1)

    act(() => {
      result.current.removeToast('2')
    })
    expect(result.current.toasts).toEqual([])
  })

  test('should trigger toast using showToast helper', () => {

    if (typeof crypto === 'undefined' || !crypto.randomUUID) {
      vi.stubGlobal('crypto', {
        randomUUID: () => '12345678-1234-1234-1234-1234567890ab',
      })
    } else {
      vi.spyOn(crypto, 'randomUUID').mockReturnValue('12345678-1234-1234-1234-1234567890ab')
    }

    const { result } = renderHook(() => useToast())

    act(() => {
      showToast('My success message', 'success')
    })

    expect(result.current.toasts).toEqual([
      { id: '12345678-1234-1234-1234-1234567890ab', message: 'My success message', type: 'success' },
    ])
  })
})
