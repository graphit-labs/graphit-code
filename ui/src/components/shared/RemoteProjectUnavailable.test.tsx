import { render, screen } from '@testing-library/react'
import { beforeEach, expect, it } from 'vitest'
import { useAppStore } from '@/store/appStore'
import { LocalProjectRoute } from './RemoteProjectUnavailable'

beforeEach(() => useAppStore.setState({
  activeProjectKey: 'workspace:local:/project', activeProjectOrigin: 'workspace', activeProjectId: 'local',
  activeProjectDir: '/project', projectName: 'Local',
}))

it('keeps a Hub target selected and blocks checkout-only content', () => {
  useAppStore.setState({
    activeProjectKey: 'hub:01HUB', activeProjectOrigin: 'hub', activeProjectId: '01HUB',
    activeProjectDir: '', projectName: 'Remote service',
  })
  render(<LocalProjectRoute capability="Hub publication"><div>unsafe local content</div></LocalProjectRoute>)
  expect(screen.queryByText('unsafe local content')).toBeNull()
  expect(screen.getByRole('heading', { name: 'Local workspace required' })).toBeTruthy()
  expect(screen.getByText('Remote service')).toBeTruthy()
  expect(useAppStore.getState().activeProjectKey).toBe('hub:01HUB')
})

it('renders checkout-only content for a Workspace target', () => {
  render(<LocalProjectRoute capability="Hub publication"><div>local content</div></LocalProjectRoute>)
  expect(screen.getByText('local content')).toBeTruthy()
})
