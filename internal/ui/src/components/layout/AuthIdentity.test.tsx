import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { AuthIdentity } from './AuthIdentity';

afterEach(() => vi.unstubAllGlobals());

describe('workspace identity', () => {
  it.each([
    ['local', 'Lia', 'Local'],
    ['Broker CLI', 'Bruna', 'Company'],
  ])('shows the active %s CLI account when web auth is disabled', async (_kind, username, provider) => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: false, authenticated: true, username, provider, providers: [] }) })));
    render(<AuthIdentity />);
    expect(await screen.findByText(username)).not.toBeNull();
    expect(screen.getByLabelText('Identidade de acesso').querySelector('.auth-identity-chip')?.getAttribute('title')).toBe(`${username} · ${provider}`);
    expect(screen.queryByRole('button', { name: 'Entrar com Broker' })).toBeNull();
  });

  it('shows anonymous without offering login when web auth is disabled', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: false, authenticated: false, username: 'Anônimo', provider: '', providers: [{ name: 'Company', type: 'broker' }] }) })));
    render(<AuthIdentity />);
    expect(await screen.findByText('Anônimo')).not.toBeNull();
    expect(screen.queryByRole('button', { name: /Entrar/ })).toBeNull();
  });

  it('starts login directly for the only Broker provider', async () => {
    let finishLogin: (response: { ok: boolean; text: () => Promise<string> }) => void = () => {};
    const loginResponse = new Promise<{ ok: boolean; text: () => Promise<string> }>(resolve => { finishLogin = resolve; });
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ enabled: true, authenticated: false, username: 'Anônimo', provider: '', providers: [{ name: 'Company', type: 'broker' }, { name: 'Local', type: 'local' }] }) })
      .mockImplementationOnce(() => loginResponse);
    vi.stubGlobal('fetch', fetchMock);
    render(<AuthIdentity />);
    const login = await screen.findByRole('button', { name: 'Entrar com Company' });
    expect(screen.queryByRole('menuitem')).toBeNull();
    await userEvent.setup().click(login);
    expect(login.hasAttribute('disabled')).toBe(true);
    expect(fetchMock).toHaveBeenLastCalledWith('/api/auth/login', expect.objectContaining({ method: 'POST', body: JSON.stringify({ provider: 'Company' }) }));
    finishLogin({ ok: false, text: async () => 'Broker unavailable' });
    expect((await screen.findByRole('alert')).textContent).toBe('Broker unavailable');
    await waitFor(() => expect(login.hasAttribute('disabled')).toBe(false));
  });

  it('offers a dropdown only when multiple Broker providers are available', async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce({ ok: true, json: async () => ({ enabled: true, authenticated: false, username: 'Anônimo', provider: '', providers: [{ name: 'Company', type: 'broker' }, { name: 'Partner', type: 'broker' }, { name: 'Local', type: 'local' }] }) })
      .mockResolvedValueOnce({ ok: false, text: async () => 'Login unavailable' });
    vi.stubGlobal('fetch', fetchMock);
    render(<AuthIdentity />);
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Entrar com Broker' }));
    expect(await screen.findByRole('menuitem', { name: 'Company' })).not.toBeNull();
    expect(screen.getByRole('menuitem', { name: 'Partner' })).not.toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Local' })).toBeNull();
    await userEvent.setup().click(screen.getByRole('menuitem', { name: 'Partner' }));
    expect(fetchMock).toHaveBeenLastCalledWith('/api/auth/login', expect.objectContaining({ method: 'POST', body: JSON.stringify({ provider: 'Partner' }) }));
  });

  it('does not offer login without a Broker provider', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: true, authenticated: false, username: 'Anônimo', provider: '', providers: [{ name: 'Local', type: 'local' }] }) })));
    render(<AuthIdentity />);
    expect(await screen.findByText('Anônimo')).not.toBeNull();
    expect(screen.queryByRole('button', { name: /Entrar/ })).toBeNull();
  });

  it('shows verified identity and logout after login', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: true, authenticated: true, username: 'Alice', provider: 'Company', providers: [{ name: 'Company', type: 'broker' }] }) })));
    render(<AuthIdentity />);
    await waitFor(() => expect(screen.getByText('Alice')).not.toBeNull());
    expect(screen.getByRole('button', { name: 'Sair da conta' })).not.toBeNull();
    expect(screen.queryByRole('button', { name: 'Entrar com Broker' })).toBeNull();
  });
});
