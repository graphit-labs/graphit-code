import { useLocation, Link } from "react-router-dom";
import { ChevronRight, ArrowUpRight } from "lucide-react";
import { Sidebar, MobileSidebar } from "./Sidebar";
import { ToastContainer } from "@/components/shared/Toast";
import { WorkspaceRefreshProvider } from "./WorkspaceRefresh";
import { WorkspaceSelectors } from "./WorkspaceSelectors";

export function AppShell({ children }: { children: React.ReactNode }) {
  return <WorkspaceRefreshProvider><ShellLayout>{children}</ShellLayout></WorkspaceRefreshProvider>;
}

function ShellLayout({ children }: { children: React.ReactNode }) {
  const { pathname } = useLocation();
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
          <WorkspaceSelectors />
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
          {children}
        </main>
      </div>
      <ToastContainer />
    </div>
  );
}
