import { describe, expect, it } from 'vitest'
import { TYPE_ICONS, TYPE_STYLES } from '../hub/ArtifactCard.constants'
import { TYPE_FILTERS } from './Sidebar.constants'

describe('Hub artifact type presentation', () => {
  it('does not expose the removed framework artifact type', () => {
    expect(TYPE_FILTERS.map(({ value }) => value)).not.toContain('framework')
    expect(TYPE_FILTERS.map(({ label }) => label)).not.toContain('Framework')
    expect(TYPE_ICONS).not.toHaveProperty('framework')
    expect(TYPE_STYLES).not.toHaveProperty('framework')
  })
})
