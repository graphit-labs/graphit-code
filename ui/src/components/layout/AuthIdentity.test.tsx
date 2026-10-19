import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AuthIdentity } from './AuthIdentity'
import { useAppStore } from '@/store/appStore'
afterEach(() => vi.unstubAllGlobals())
const response = (status: number, value: unknown = {}) => ({ ok: status >= 200 && status < 300, status, json: async () => value, text: async () => 'Login unavailable' })
function setup(identity: ReturnType<typeof response>, providers = ['Company']) {
 const fetcher = vi.fn(async (url: string) => url === '/auth/session' ? identity : url === '/auth/providers' ? response(200, { providers }) : response(503))
 vi.stubGlobal('fetch', fetcher)
 return fetcher
}
describe('native Browser identity', () => {
 it('offers login from independent providers when cookie identity is 401 anonymous', async () => {
  const fetcher = setup(response(401)); render(<AuthIdentity />)
  expect(await screen.findByRole('button', { name: 'Entrar com Company' })).toBeTruthy()
  expect(screen.getByText('Anônimo')).toBeTruthy()
  expect(useAppStore.getState().webIdentity).toEqual({ state: 'anonymous' })
  expect(fetcher.mock.calls.map(call => call[0]).sort()).toEqual(['/auth/providers', '/auth/session'])
 })
 it('shows verified nullable username and logout without UserInfo or CLI identity', async () => {
  const fetcher = setup(response(200, { provider: 'Company', subject: 'fixture', username: null, expires_at: 99 })); render(<AuthIdentity />)
  expect(await screen.findByText('Conta autenticada')).toBeTruthy()
  expect(screen.getByRole('button', { name: 'Sair da conta' })).toBeTruthy()
  expect(useAppStore.getState().webIdentity).toEqual({ state: 'authenticated', provider: 'Company', subject: 'fixture' })
  expect(fetcher).toHaveBeenCalledTimes(2)
 })
 it('keeps a failed identity distinct from anonymous and supports retry', async () => {
  const fetcher = setup(response(503)); render(<AuthIdentity />)
  expect(await screen.findByRole('alert')).toBeTruthy()
  await userEvent.setup().click(screen.getByRole('button', { name: 'Tentar novamente' }))
  await waitFor(() => expect(fetcher).toHaveBeenCalledTimes(4))
 })
 it('starts the native JSON login, clears busy on failure and preserves exact provider', async () => {
  const fetcher = setup(response(401)); render(<AuthIdentity />)
  const button = await screen.findByRole('button', { name: 'Entrar com Company' })
  await userEvent.setup().click(button)
  await screen.findByRole('alert')
  expect(fetcher).toHaveBeenLastCalledWith('/auth/login', expect.objectContaining({ method: 'POST', credentials: 'same-origin', body: JSON.stringify({ provider: 'Company' }) }))
  expect(button.hasAttribute('disabled')).toBe(false)
 })
 it('uses the existing provider menu only for multiple native names', async () => {
  setup(response(401), ['Company', 'Partner']); render(<AuthIdentity />)
  await userEvent.setup().click(await screen.findByRole('button', { name: 'Entrar com Broker' }))
  expect(screen.getByRole('menuitem', { name: 'Company' })).toBeTruthy()
  expect(screen.getByRole('menuitem', { name: 'Partner' })).toBeTruthy()
 })
 it('aborts both native identity/provider requests when unmounted', async () => {
  const fetcher = vi.fn((_url: string, _init?: RequestInit) => new Promise(() => {})); vi.stubGlobal('fetch', fetcher)
  const view = render(<AuthIdentity />); view.unmount()
  expect(fetcher.mock.calls.every(call => call[1]?.signal?.aborted)).toBe(true)
 })
})
