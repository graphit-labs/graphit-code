import { vi } from 'vitest'
// Browser APIs used by Radix's focus, positioning and pointer handling.
vi.stubGlobal('PointerEvent', MouseEvent)
vi.stubGlobal('ResizeObserver', class { observe() {} unobserve() {} disconnect() {} })
HTMLElement.prototype.hasPointerCapture = () => false
HTMLElement.prototype.setPointerCapture = () => {}
HTMLElement.prototype.releasePointerCapture = () => {}
HTMLElement.prototype.scrollIntoView = vi.fn()
