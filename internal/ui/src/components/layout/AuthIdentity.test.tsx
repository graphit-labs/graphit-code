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
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: false, authenticated: false, username: 'Anônimo', provider: '', providers: [] }) })));
    render(<AuthIdentity />);
    expect(await screen.findByText('Anônimo')).not.toBeNull();
    expect(screen.queryByRole('button', { name: 'Entrar com Broker' })).toBeNull();
  });

  it('offers only Broker providers and identifies the authenticated account', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: true, authenticated: false, username: 'Anônimo', provider: '', providers: [{ name: 'Company', type: 'broker' }, { name: 'Local', type: 'local' }] }) })));
    render(<AuthIdentity />);
    const login = await screen.findByRole('button', { name: 'Entrar com Broker' });
    await userEvent.setup().click(login);
    expect(await screen.findByRole('menuitem', { name: 'Company' })).not.toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Local' })).toBeNull();
  });

  it('shows verified identity and logout after login', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => ({ enabled: true, authenticated: true, username: 'Alice', provider: 'Company', providers: [{ name: 'Company', type: 'broker' }] }) })));
    render(<AuthIdentity />);
    await waitFor(() => expect(screen.getByText('Alice')).not.toBeNull());
    expect(screen.getByRole('button', { name: 'Sair da conta' })).not.toBeNull();
    expect(screen.queryByRole('button', { name: 'Entrar com Broker' })).toBeNull();
  });
});
