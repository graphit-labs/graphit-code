import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { AppShell } from "./components/layout/AppShell";
import { ThemeProvider } from "./components/layout/ThemeProvider";
import { useAppStore } from "./store/appStore";
import { lazy, Suspense, useEffect } from "react";
import { LoadingSpinner } from "./components/shared/LoadingSpinner";
import { GlobalLoader } from "./components/shared/GlobalLoader";
import { LocalProjectRoute } from "./components/shared/RemoteProjectUnavailable";
import { agentFeaturesEnabled } from "@/lib/utils";

const WorkspacePage = lazy(() => import("./components/system/WorkspacePage"));
const RegistryPage = lazy(() => import("./components/hub/RegistryPage"));
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
      <WikiExplorerPage autoSelectProject />
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

function WikiSearchResultsWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <WikiExplorerPage autoSelectProject />
    </Suspense>
  );
}

function DefaultRedirect() {
  const { appMode } = useAppStore();
  return (
    <Navigate to={appMode === "ast" ? "/ast/contexts" : "/workspace"} replace />
  );
}

function KnowledgeContextsPage() {
  return <WikiContextsPage moduleFilter="knowledge" />;
}

export default function App() {
  useEffect(() => {
    useAppStore.getState().loadProjects();
  }, []);

  return (
    <ThemeProvider>
      <BrowserRouter>
        <GlobalLoader />
        <AppShell>
          <Suspense fallback={<Fallback />}>
            <Routes>
              <Route
                path="/ast/explorer/:contextId"
                element={<ASTExplorerWrapper />}
              />
              <Route path="/ast/explorer" element={<ASTExplorerWrapper />} />
              <Route
                path="/knowledge/explorer/:moduleId"
                element={<KnowledgeExplorerWrapper />}
              />
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
                path="/wiki/explorer"
                element={<WikiSearchResultsWrapper />}
              />
              <Route
                path="/task/explorer/:taskId"
                element={<TaskExplorerPage />}
              />
              <Route path="/task/explorer" element={<TaskExplorerPage />} />
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
                      <Route path="/workspace" element={<LocalProjectRoute capability="Workspace now"><WorkspacePage /></LocalProjectRoute>} />
                      <Route path="/hub/registry" element={<LocalProjectRoute capability="Hub registry operations"><RegistryPage /></LocalProjectRoute>} />
                      <Route
                        path="/hub/local"
                        element={<LocalProjectRoute capability="Local project artifacts"><ProjectArtifactsPage /></LocalProjectRoute>}
                      />
                      <Route path="/hub/upload" element={<LocalProjectRoute capability="Hub publication"><UploadPage /></LocalProjectRoute>} />
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
            </Routes>
          </Suspense>
        </AppShell>
      </BrowserRouter>
    </ThemeProvider>
  );
}
