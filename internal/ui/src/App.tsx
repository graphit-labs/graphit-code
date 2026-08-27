import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { AppShell } from './components/layout/AppShell'
import { ThemeProvider } from './components/layout/ThemeProvider'
import { useAppStore } from './store/appStore'
import { lazy, Suspense, useEffect } from 'react'
import { LoadingSpinner } from './components/shared/LoadingSpinner'
import { GlobalLoader } from './components/shared/GlobalLoader'

const RegistryPage = lazy(() => import('./components/hub/RegistryPage'))
const ProjectArtifactsPage = lazy(() => import('./components/hub/ProjectArtifactsPage'))
const UploadPage = lazy(() => import('./components/hub/UploadPage'))
const ContextsPage = lazy(() => import('./components/ast/ContextsPage'))
const ExplorerPage = lazy(() => import('./components/ast/ExplorerPage'))
const WikiExplorerPage = lazy(() => import('./components/wiki/WikiExplorerPage'))
const WikiContextsPage = lazy(() => import('./components/wiki/WikiContextsPage'))
const LiveSearchPage = lazy(() => import('./components/live/LiveSearchPage'))
const DaemonDashboard = lazy(() => import('./components/daemon/DaemonDashboard'))
const DreamDashboard = lazy(() => import('./components/dream/DreamDashboard'))
const EcosystemDashboard = lazy(() => import('./components/system/EcosystemDashboard'))

function Fallback() {
  return (
    <div className="flex items-center justify-center h-full min-h-[200px]">
      <LoadingSpinner size="md" />
    </div>
  )
}

function ASTExplorerWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <ExplorerPage />
    </Suspense>
  )
}

function KnowledgeExplorerWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <WikiExplorerPage moduleFilter="knowledge" autoSelectProject />
    </Suspense>
  )
}

function MemoryExplorerWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <WikiExplorerPage moduleFilter="memory" />
    </Suspense>
  )
}

function WikiSearchResultsWrapper() {
  return (
    <Suspense fallback={<Fallback />}>
      <WikiExplorerPage autoSelectProject />
    </Suspense>
  )
}

function DefaultRedirect() {
  const { appMode } = useAppStore()
  return <Navigate to={appMode === 'ast' ? '/ast/contexts' : '/hub/registry'} replace />
}

function KnowledgeContextsPage() {
  return <WikiContextsPage moduleFilter="knowledge" />
}

export default function App() {
  useEffect(() => {
    useAppStore.getState().loadProjects()
  }, [])

  return (
    <ThemeProvider>
      <BrowserRouter>
        <GlobalLoader />
        <Routes>
          
          <Route path="/ast/explorer/:contextId" element={<ASTExplorerWrapper />} />
          <Route path="/ast/explorer" element={<ASTExplorerWrapper />} />
          <Route path="/knowledge/explorer/:moduleId" element={<KnowledgeExplorerWrapper />} />
          <Route path="/knowledge/explorer" element={<KnowledgeExplorerWrapper />} />
          <Route path="/memory/explorer/:moduleId" element={<MemoryExplorerWrapper />} />
          <Route path="/memory/explorer" element={<MemoryExplorerWrapper />} />
          <Route path="/wiki/explorer" element={<WikiSearchResultsWrapper />} />

          
          <Route
            path="/*"
            element={
              <AppShell>
                <Suspense fallback={<Fallback />}>
                  <Routes>
                    <Route path="/hub/registry" element={<RegistryPage />} />
                    <Route path="/hub/local" element={<ProjectArtifactsPage />} />
                    <Route path="/hub/upload" element={<UploadPage />} />
                    <Route path="/ast/contexts" element={<ContextsPage />} />
                    <Route path="/knowledge/contexts" element={<KnowledgeContextsPage />} />
                    <Route path="/live" element={<LiveSearchPage />} />
                    <Route path="/wiki" element={<Navigate to="/live" replace />} />
                    <Route path="/system/daemon" element={<DaemonDashboard />} />
                    <Route path="/system/dream" element={<DreamDashboard />} />
                    <Route path="/system/ecosystem" element={<EcosystemDashboard />} />
                    <Route path="*" element={<DefaultRedirect />} />
                  </Routes>
                </Suspense>
              </AppShell>
            }
          />
        </Routes>
      </BrowserRouter>
    </ThemeProvider>
  )
}
