import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import { ChevronDown, CircleUserRound, LogIn, LogOut } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useAppStore } from '@/store/appStore';

type AuthSession = { username: string | null; provider: string; subject: string; expires_at: number };

export function AuthIdentity() {
  const [session, setSession] = useState<AuthSession | null>(null);
  const [providers, setProviders] = useState<string[]>([]);
  const [retry, setRetry] = useState(0);
  const action = useRef<AbortController | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    const controller = new AbortController();
    useAppStore.getState().setWebIdentity({ state: 'loading' });
    const init = { credentials: 'same-origin' as const, cache: 'no-store' as const, signal: controller.signal };
    const identity = fetch('/auth/session', init).then(async response => {
      if (response.status === 401) return null;
      if (!response.ok) throw new Error('Não foi possível consultar a sessão');
      return response.json() as Promise<AuthSession>;
    }).then(value => {
      if (controller.signal.aborted) return;
      setSession(value);
      useAppStore.getState().setWebIdentity(value ? { state: 'authenticated', provider: value.provider, subject: value.subject } : { state: 'anonymous' });
    }, cause => {
      if (!controller.signal.aborted) useAppStore.getState().setWebIdentity({ state: 'unavailable' });
      throw cause;
    });
    const available = fetch('/auth/providers', init).then(async response => {
      if (!response.ok) throw new Error('Não foi possível consultar os provedores');
      return response.json() as Promise<{ providers: string[] }>;
    }).then(value => { if (!controller.signal.aborted) setProviders(value.providers); });
    void Promise.allSettled([identity, available]).then(results => {
      if (controller.signal.aborted) return;
      const failure = results.find(result => result.status === 'rejected');
      if (failure?.status === 'rejected') setError(failure.reason instanceof Error ? failure.reason.message : 'Não foi possível consultar a conta');
      setLoading(false);
    });
    return () => { controller.abort(); action.current?.abort(); };
  }, [retry]);

  async function startLogin(provider: string) {
    action.current?.abort();
    const controller = new AbortController(); action.current = controller;
    setBusy(true);
    setError('');
    try {
      const response = await fetch('/auth/login', {
        method: 'POST', credentials: 'same-origin', signal: controller.signal,
        headers: { 'Content-Type': 'application/json', 'X-Graphit-Request': 'ui' },
        body: JSON.stringify({ provider }),
      });
      if (!response.ok) throw new Error('Não foi possível iniciar o login (HTTP ' + response.status + ')');
      const result = await response.json() as { authorization_url: string };
      if (!controller.signal.aborted) window.location.assign(result.authorization_url);
    } catch (cause) {
      if (controller.signal.aborted) return;
      setError(cause instanceof Error ? cause.message : 'Não foi possível iniciar o login');
      setBusy(false);
    }
  }

  async function logout() {
    action.current?.abort();
    const controller = new AbortController(); action.current = controller;
    setBusy(true);
    setError('');
    try {
      const response = await fetch('/auth/logout', { method: 'POST', credentials: 'same-origin', signal: controller.signal, headers: { 'X-Graphit-Request': 'ui' } });
      if (!response.ok) throw new Error('Não foi possível sair');
      if (!controller.signal.aborted) {
        useAppStore.getState().setWebIdentity({ state: 'anonymous' });
        window.location.assign('/task/gantt');
      }
    } catch (cause) {
      if (controller.signal.aborted) return;
      setError(cause instanceof Error ? cause.message : 'Não foi possível sair');
      setBusy(false);
    }
  }

  const name = session ? session.username || 'Conta autenticada' : 'Anônimo';
  return <div className="auth-identity" aria-label="Identidade de acesso">
    <div className="auth-identity-chip" aria-live="polite" title={session !== null ? `${name} · ${session?.provider}` : name}>
      <CircleUserRound size={17} aria-hidden="true" />
      <span className="auth-identity-copy"><small>Conta</small><strong>{loading ? 'Carregando…' : name}</strong></span>
    </div>
    {!loading && (session !== null ?
      <button className="auth-identity-action" type="button" onClick={() => void logout()} disabled={busy} aria-label="Sair da conta"><LogOut size={16} aria-hidden="true" /><span>Sair</span></button>
      : providers.length === 1 ?
        <button className="auth-identity-action" type="button" onClick={() => void startLogin(providers[0])} disabled={busy} aria-label={`Entrar com ${providers[0]}`}>
          <LogIn size={16} aria-hidden="true" /><span>Entrar</span>
        </button>
      : providers.length > 1 && <DropdownMenu.Root>
        <DropdownMenu.Trigger className="auth-identity-action" disabled={busy} aria-label="Entrar com Broker">
          <LogIn size={16} aria-hidden="true" /><span>Entrar</span><ChevronDown size={13} aria-hidden="true" />
        </DropdownMenu.Trigger>
        <DropdownMenu.Portal><DropdownMenu.Content className="auth-provider-menu" sideOffset={8} align="end">
          <DropdownMenu.Label className="auth-provider-heading">Entrar com Broker</DropdownMenu.Label>
          {providers.map(provider => <DropdownMenu.Item key={provider} className="auth-provider-item" onSelect={() => void startLogin(provider)}>{provider}</DropdownMenu.Item>)}
        </DropdownMenu.Content></DropdownMenu.Portal>
      </DropdownMenu.Root>)}
    {error && <span className="auth-identity-error" role="alert">{error}<button className="auth-identity-action" type="button" onClick={() => { setLoading(true); setError(''); setRetry(value => value + 1); }} disabled={busy}>Tentar novamente</button></span>}
  </div>;
}
