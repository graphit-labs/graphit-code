import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import { ChevronDown, CircleUserRound, LogIn, LogOut } from 'lucide-react';
import { useEffect, useState } from 'react';

type AuthSession = {
  enabled: boolean;
  authenticated: boolean;
  username: string;
  provider: string;
  providers: { name: string; type: string }[];
};

const anonymous: AuthSession = { enabled: false, authenticated: false, username: 'Anônimo', provider: '', providers: [] };

export function AuthIdentity() {
  const [session, setSession] = useState<AuthSession>(anonymous);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    let live = true;
    fetch('/api/auth/session', { credentials: 'same-origin', cache: 'no-store' })
      .then(async response => {
        if (!response.ok) throw new Error('Não foi possível consultar a sessão');
        return response.json() as Promise<AuthSession>;
      })
      .then(value => { if (live) setSession(value); })
      .catch(() => { if (live) { setSession(anonymous); setError('Não foi possível consultar a sessão. Recarregue a página.'); } })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, []);

  async function startLogin(provider: string) {
    setBusy(true);
    setError('');
    try {
      const response = await fetch('/api/auth/login', {
        method: 'POST', credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json', 'X-Graphit-Request': 'ui' },
        body: JSON.stringify({ provider }),
      });
      if (!response.ok) throw new Error((await response.text()).trim() || 'Não foi possível iniciar o login');
      const result = await response.json() as { authorization_url: string };
      window.location.assign(result.authorization_url);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Não foi possível iniciar o login');
      setBusy(false);
    }
  }

  async function logout() {
    setBusy(true);
    setError('');
    try {
      const response = await fetch('/api/auth/logout', { method: 'POST', credentials: 'same-origin', headers: { 'X-Graphit-Request': 'ui' } });
      if (!response.ok) throw new Error('Não foi possível sair');
      window.location.assign('/workspace');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Não foi possível sair');
      setBusy(false);
    }
  }

  const providers = session.providers.filter(provider => provider.type === 'broker');
  const name = session.authenticated ? session.username || 'Conta autenticada' : 'Anônimo';
  return <div className="auth-identity" aria-label="Identidade de acesso">
    <div className="auth-identity-chip" aria-live="polite" title={session.authenticated ? `${name} · ${session.provider}` : name}>
      <CircleUserRound size={17} aria-hidden="true" />
      <span className="auth-identity-copy"><small>Conta</small><strong>{loading ? 'Carregando…' : name}</strong></span>
    </div>
    {session.enabled && (session.authenticated ?
      <button className="auth-identity-action" type="button" onClick={() => void logout()} disabled={busy} aria-label="Sair da conta"><LogOut size={16} aria-hidden="true" /><span>Sair</span></button>
      : providers.length > 0 && <DropdownMenu.Root>
        <DropdownMenu.Trigger className="auth-identity-action" disabled={busy} aria-label="Entrar com Broker">
          <LogIn size={16} aria-hidden="true" /><span>Entrar</span><ChevronDown size={13} aria-hidden="true" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal><DropdownMenu.Content className="auth-provider-menu" sideOffset={8} align="end">
          <DropdownMenu.Label className="auth-provider-heading">Entrar com Broker</DropdownMenu.Label>
          {providers.map(provider => <DropdownMenu.Item key={provider.name} className="auth-provider-item" onSelect={() => void startLogin(provider.name)}>{provider.name}</DropdownMenu.Item>)}
        </DropdownMenu.Content></DropdownMenu.Portal>
      </DropdownMenu.Root>)}
    {error && <span className="auth-identity-error" role="alert">{error}</span>}
  </div>;
}
