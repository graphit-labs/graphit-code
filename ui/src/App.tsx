import { BrowserRouter, Routes, Route, Navigate, useLocation } from "react-router-dom";
import { AppShell } from "./components/layout/AppShell";
import { ThemeProvider } from "./components/layout/ThemeProvider";
import { useAppStore } from "./store/appStore";
import { useProviderMode } from "./store/providerMode";
import { lazy, Suspense, useEffect } from "react";
import { LoadingSpinner } from "./components/shared/LoadingSpinner";
import { GlobalLoader } from "./components/shared/GlobalLoader";
import { WorkNotice } from './components/shared/EngineeringUI';
import { LocalProjectRoute } from "./components/shared/RemoteProjectUnavailable";
import { agentFeaturesEnabled } from "@/lib/utils";
import { TaskWorkspaceBootstrap } from "./components/task/TaskWorkspace";
import { fileHubRoute, importedKnowledgeLocation, installedHubRoute, projectOperationsRoute, projectReadModule, sharedEcosystemRoute, userMemoryRoute } from '@/api/nativeRecordNavigation';

const WorkspacePage = lazy(() => import("./components/system/WorkspacePage"));
const RegistryPage = lazy(() => import("./components/hub/RegistryPage"));
const InstalledLibrariesPage = lazy(() => import("./components/hub/InstalledLibrariesPage"));
const ProjectArtifactsPage = lazy(
  () => import("./components/hub/ProjectArtifactsPage"),
);
const UploadPage = lazy(() => import("./components/hub/UploadPage"));
const ContextsPage = lazy(() => import("./components/ast/ContextsPage"));
const ExplorerPage = lazy(() => import("./components/ast/ExplorerPage"));
const WikiExplorerPage = lazy(
  () => import("./components/wiki/WikiExplorerPage"),
);
const WikiContextsPage = lazy(
  () => import("./components/wiki/WikiContextsPage"),
);
const LiveSearchPage = lazy(() => import("./components/live/LiveSearchPage"));
const DaemonDashboard = lazy(
  () => import("./components/daemon/DaemonDashboard"),
);
const DreamDashboard = lazy(() => import("./components/dream/DreamDashboard"));
const EcosystemDashboard = lazy(
  () => import("./components/system/EcosystemDashboard"),
);
const TaskGanttPage = lazy(() => import('./components/task/TaskGanttPage'))
const TaskExplorerPage = lazy(
  () => import("./components/task/TaskExplorerPage"),
);
const SessionExplorerPage = lazy(
  () => import("./components/task/SessionExplorerPage"),
);
const MemoryExplorerPage = lazy(
  () => import("./components/memory/MemoryExplorerPage"),
);

function Fallback() {
  return (
    <div className="flex items-center justify-center h-full min-h-[200px]">
      <LoadingSpinner size="md" />
    </div>
  );
}

function ASTExplorerWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <ExplorerPage />
    </Suspense>
  );
}

function KnowledgeExplorerWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <WikiExplorerPage />
    </Suspense>
  );
}

function MemoryExplorerWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <MemoryExplorerPage />
    </Suspense>
  );
}

function DefaultRedirect() {
  const { appMode } = useAppStore();
  const provider = useProviderMode();
  return (
    <Navigate to={appMode === "ast" ? provider === 'local' ? '/ast/explorer' : "/ast/contexts" : "/workspace"} replace />
  );
}

function KnowledgeContextsPage() {
  return <WikiContextsPage />;
}

function RouteWorkspace({ children }: { children: React.ReactNode }) {
  const { pathname, search } = useLocation();
  const provider = useProviderMode();
  const identity = useAppStore(state => state.webIdentity);
  const shared = sharedEcosystemRoute(pathname, search);
  const module = projectReadModule(pathname, search, provider === 'local');
  const independent = installedHubRoute(pathname) || userMemoryRoute(pathname) || importedKnowledgeLocation(pathname, search) || (shared && !module);
  const awaitingProvider = provider === 'loading' && (shared || projectOperationsRoute(pathname));
  const localOnly = shared && provider === 'local';
  useEffect(() => { if (!awaitingProvider && !localOnly && !module && !independent) void useAppStore.getState().loadProjects(); }, [module, independent, awaitingProvider, localOnly]);
  if (awaitingProvider) return <Fallback />;
  if (shared && provider === 'unavailable') return <p role="alert">The workspace provider is unavailable. Check the account menu and retry.</p>;
  if (localOnly) return <Navigate replace to={pathname.startsWith('/ast') ? '/ast/explorer' : pathname.startsWith('/knowledge') ? '/knowledge/explorer' : '/workspace'} />;
  if (shared && provider === 'broker' && identity.state === 'anonymous' && !pathname.startsWith('/hub/')) {
    return <WorkNotice title="Sign in to browse shared contexts">Use the account control in the header.</WorkNotice>;
  }
  return module ? <TaskWorkspaceBootstrap module={module}>{children}</TaskWorkspaceBootstrap> : children;
}

function WorkspaceRoutes({ children }: { children: React.ReactNode }) {
  const { pathname, search } = useLocation();
  const activeProjectVersion = useAppStore((state) => state.activeProjectVersion);
  const provider = useProviderMode();
  const module = projectReadModule(pathname, search, provider === 'local');
  const independent = fileHubRoute(pathname) || installedHubRoute(pathname) || userMemoryRoute(pathname) || importedKnowledgeLocation(pathname, search) || (sharedEcosystemRoute(pathname, search) && !module);
  return <Routes key={module ? 'native-' + module : independent ? 'native-independent' : `version:${activeProjectVersion}`}>{children}</Routes>;
}

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <GlobalLoader />
        <AppShell>
          <RouteWorkspace>
          <Suspense fallback={<Fallback />}>
            <WorkspaceRoutes>
              <Route
                path="/ast/explorer/:contextId"
                element={<ASTExplorerWrapper />}
              />
              <Route path="/ast/explorer" element={<ASTExplorerWrapper />} />
              <Route
                path="/knowledge/explorer"
                element={<KnowledgeExplorerWrapper />}
              />
              <Route
                path="/memory/explorer/:scopeId"
                element={<MemoryExplorerWrapper />}
              />
              <Route
                path="/memory/explorer/:scopeId/:memoryId"
                element={<MemoryExplorerWrapper />}
              />
              <Route
                path="/memory/explorer"
                element={<MemoryExplorerWrapper />}
              />
              <Route
                path="/task/explorer/:taskId"
                element={<TaskExplorerPage />}
              />
              <Route path="/task/explorer" element={<TaskExplorerPage />} />
              <Route path="/task/gantt" element={<TaskGanttPage />} />
              <Route
                path="/task/sessions/:sessionId"
                element={<SessionExplorerPage />}
              />
              <Route path="/task/sessions" element={<SessionExplorerPage />} />

              <Route
                path="/*"
                element={
                  <Suspense fallback={<Fallback />}>
                    <Routes>
                      <Route path="/workspace" element={<WorkspacePage />} />
                      <Route path="/hub/installed" element={<InstalledLibrariesPage />} />
                      <Route path="/hub/registry" element={<RegistryPage />} />
                      <Route
                        path="/hub/local"
                        element={<ProjectArtifactsPage />}
                      />
                      <Route path="/hub/upload" element={<UploadPage />} />
                      <Route path="/ast/contexts" element={<ContextsPage />} />
                      <Route
                        path="/knowledge/contexts"
                        element={<KnowledgeContextsPage />}
                      />
                      <Route
                        path="/live"
                        element={
                          agentFeaturesEnabled() ? (
                            <LocalProjectRoute capability="Live workspace search"><LiveSearchPage /></LocalProjectRoute>
                          ) : (
                            <Navigate to="/hub/registry" replace />
                          )
                        }
                      />
                      <Route
                        path="/wiki"
                        element={
                          <Navigate
                            to={
                              agentFeaturesEnabled()
                                ? "/live"
                                : "/knowledge/explorer"
                            }
                            replace
                          />
                        }
                      />
                      <Route
                        path="/system/daemon"
                        element={<DaemonDashboard />}
                      />
                      <Route
                        path="/system/dream"
                        element={<LocalProjectRoute capability="Dream workspace analysis"><DreamDashboard /></LocalProjectRoute>}
                      />
                      <Route
                        path="/system/ecosystem"
                        element={<EcosystemDashboard />}
                      />
                      <Route path="*" element={<DefaultRedirect />} />
                    </Routes>
                  </Suspense>
                }
              />
            </WorkspaceRoutes>
          </Suspense>
          </RouteWorkspace>
        </AppShell>
      </BrowserRouter>
    </ThemeProvider>
  );
}
