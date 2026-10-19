import '@testing-library/jest-dom/vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '@/store/appStore'
import { Sidebar } from './Sidebar'
import { TYPE_ICONS, TYPE_STYLES } from '../hub/ArtifactCard.constants'
import { TYPE_FILTERS } from './Sidebar.constants'

vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light', toggle: vi.fn() }) }))

describe('Hub artifact type presentation', () => {
  it('does not expose the removed framework artifact type', () => {
    expect(TYPE_FILTERS.map(({ value }) => value)).not.toContain('framework')
    expect(TYPE_FILTERS.map(({ label }) => label)).not.toContain('Framework')
    expect(TYPE_ICONS).not.toHaveProperty('framework')
    expect(TYPE_STYLES).not.toHaveProperty('framework')
  })
})

describe('FILE Hub Web navigation', () => {
  beforeEach(() => useAppStore.setState({ webMode: true, webIdentity: { state: 'authenticated', provider: 'corporate', subject: 'navigation-user' } }))
  afterEach(cleanup)
  it.each(['/hub/registry', '/hub/local'])('exposes Project artifacts and omits the unused global type filter on %s', path => {
    render(<MemoryRouter initialEntries={[path]}><Sidebar /></MemoryRouter>)
    expect(screen.getByRole('link', { name: 'Project artifacts' })).toHaveAttribute('href', '/hub/local')
    expect(screen.getByRole('link', { name: 'Installed libraries' })).toHaveAttribute('href', '/hub/installed')
    expect(screen.queryByText('Artifact type')).not.toBeInTheDocument()
  })
})
