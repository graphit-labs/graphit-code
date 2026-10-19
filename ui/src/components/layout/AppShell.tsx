import { useLocation, Link } from "react-router-dom";
import { ChevronRight, ArrowUpRight, RefreshCw } from "lucide-react";
import { Sidebar, MobileSidebar } from "./Sidebar";
import { ToastContainer } from "@/components/shared/Toast";
import { WorkspaceRefreshProvider } from "./WorkspaceRefreshProvider";
import { WorkspaceSelectors } from "./WorkspaceSelectors";
import { AuthIdentity } from "./AuthIdentity";
import { AuthCallbackNotice } from "./AuthCallbackNotice";
import { TaskWorkspaceSelectors } from "../task/TaskWorkspace";
import { useWorkspaceRefresh } from './WorkspaceRefresh';
import { importedKnowledgeLocation, installedHubRoute, projectReadModule, userMemoryRoute } from '@/api/nativeRecordNavigation';

function IndependentWorkspaceControls({ label, context }: { label: string; context: string }) {
  const refresh = useWorkspaceRefresh();
  return <div className="workspace-selectors" role="group" aria-label={context}>
    <span>{label}</span>
    <button className="context-refresh" type="button" onClick={() => void refresh?.refresh()} disabled={!refresh || refresh.refreshing}
      aria-label="Refresh" title="Refresh this library" aria-busy={refresh?.refreshing || false}>
      <RefreshCw size={17} className={refresh?.refreshing ? 'animate-spin' : ''} aria-hidden="true" />
    </button>
    {refresh?.error && <div className="workspace-refresh-error" role="alert">{refresh.error}</div>}
  </div>;
}

export function AppShell({ children }: { children: React.ReactNode }) {
  return <WorkspaceRefreshProvider><ShellLayout>{children}</ShellLayout></WorkspaceRefreshProvider>;
}

function ShellLayout({ children }: { children: React.ReactNode }) {
  const { pathname, search } = useLocation();
  const module = projectReadModule(pathname, search);
  const imported = importedKnowledgeLocation(pathname, search);
  const explorer = /\/explorer|\/task\/sessions/.test(pathname);
  const domain = pathname.startsWith("/task")
    ? "Work & evidence"
    : pathname.startsWith("/ast")
      ? "Code intelligence"
      : pathname.startsWith("/knowledge") || pathname.startsWith("/wiki")
        ? "Knowledge"
        : pathname.startsWith("/memory")
          ? "Memory"
          : pathname.startsWith("/hub")
            ? "Artifact distribution"
            : pathname.startsWith("/system")
              ? "Operations"
              : pathname === "/live"
                ? "Live search"
                : "Overview";
  return (
    <div className="app-frame">
      <a className="skip-link" href="#workspace-content">
        Skip to workspace
      </a>
      <aside className="desktop-navigation">
        <Sidebar />
      </aside>
      <div className="workspace-shell">
        <header className="workspace-header" aria-label="Workspace header">
          <MobileSidebar />
          <div className="workspace-breadcrumb">
            <Link to="/workspace">Workspace</Link>
            <ChevronRight size={13} />
            <span>{domain}</span>
          </div>
          <a
            className="workspace-docs"
            href="https://github.com/graphit-labs/graphit-code"
            target="_blank"
            rel="noreferrer"
          >
            Source <ArrowUpRight size={14} />
          </a>
          {module ? <TaskWorkspaceSelectors module={module} /> : installedHubRoute(pathname) || userMemoryRoute(pathname) || imported ? <IndependentWorkspaceControls label={installedHubRoute(pathname) ? 'Installed libraries' : imported ? 'Imported Knowledge' : 'Your memories'} context={installedHubRoute(pathname) ? 'Installed library context' : imported ? 'Imported Knowledge context' : 'User memory context'} /> : <WorkspaceSelectors />}
          <AuthIdentity />
        </header>
        <main
          id="workspace-content"
          tabIndex={-1}
          className={
            explorer
              ? "workspace-content workspace-explorer"
              : "workspace-content workspace-page"
          }
        >
          {pathname === "/workspace" && <AuthCallbackNotice />}
          {children}
        </main>
      </div>
      <ToastContainer />
    </div>
  );
}
