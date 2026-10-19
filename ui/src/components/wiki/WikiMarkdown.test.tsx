import { fireEvent, render, screen, within } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { MarkdownContent } from './WikiMarkdown'

vi.mock('@/hooks/useTheme', () => ({ useTheme: () => ({ theme: 'light' }) }))

it('opens an image preview by keyboard in the shared renderer and closes it by Escape', async () => {
  render(<MarkdownContent content="![Published diagram](/published/diagram.svg)" />)
  const image = screen.getByRole('button', { name: 'Published diagram' })
  fireEvent.keyDown(image, { key: 'Enter' })
  const preview = await screen.findByRole('dialog', { name: 'Image preview' })
  expect(within(preview).getByRole('img').getAttribute('src')).toBe('/published/diagram.svg')
  fireEvent.keyDown(document, { key: 'Escape' })
  expect(screen.queryByRole('dialog')).toBeNull()
})

it('opens a rendered diagram without an explorer event listener', async () => {
  render(<MarkdownContent content={'```plantuml\nAlice -> Bob\n```'} />)
  fireEvent.click(screen.getByRole('button', { name: 'PlantUML diagram' }))
  const preview = await screen.findByRole('dialog', { name: 'Image preview' })
  expect(within(preview).getByRole('img').getAttribute('src')).toContain('https://www.plantuml.com/plantuml/svg/~h')
  fireEvent.click(within(preview).getByRole('button', { name: 'Close image preview' }))
  expect(screen.queryByRole('dialog')).toBeNull()
})
