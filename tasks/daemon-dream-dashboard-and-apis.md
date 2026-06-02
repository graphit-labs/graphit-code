---
title: Add Daemon Status and Dream Module UI & APIs
status: done
created: 2026-06-02
updated: 2026-06-03
tags: [daemon, dream, ui, api, backend]
---

# Add Daemon Status and Dream Module UI & APIs

## Objective
Implement the missing background Daemon status/control APIs and UI dashboards, and the Dream module monitoring, config, reports browser, and subjects management queue. This addresses gaps #6 ("Incomplete UI Dashboard") in the Gap Analysis by exposing Go endpoints, React pages, styling, navigation, and testing.

## Implementation Details
- Exposed Go HTTP API routes in `internal/uiserver/unified_server.go` and implemented handlers in `internal/uiserver/daemon_dream_handler.go`.
- Added unit tests in `internal/uiserver/daemon_dream_handler_test.go` covering daemon status/stop, and dream status/reports/subjects endpoints.
- Added TypeScript client API definitions in `internal/ui/src/api/daemon.ts` and `internal/ui/src/api/dream.ts`.
- Built high-quality React pages:
  - `internal/ui/src/components/daemon/DaemonDashboard.tsx` — Status monitoring, PID files info, uptime, live console logs, and process signaling.
  - `internal/ui/src/components/dream/DreamDashboard.tsx` — State monitoring, idle and duration limits, reports history rendering with Markdown support, and interactive subjects queue (add/remove).
- Modified navigation sidebar in `internal/ui/src/components/layout/Sidebar.tsx` and routing in `internal/ui/src/App.tsx`.
- Resolved TypeScript hook dependencies warnings and synchronous `setState` in `useEffect` (complying with the `react-hooks/set-state-in-effect` rule via `setTimeout` microtask scheduling).

## Use Cases

### UC-01: View Daemon Status & Logs
- **Actor**: User (via UI Dashboard)
- **Preconditions**: Unified server is running.
- **Main Flow**:
  1. User navigates to the "Daemon Status" page in the sidebar.
  2. UI requests `GET /api/daemon/status` from the backend.
  3. Go backend checks PID file, parses process runtime metadata, reads the last 50 lines of `daemon.log`, and returns the payload.
  4. React UI displays running state, process ID, uptime, and displays log records in a scrollable console frame.
- **Alternative Flows**:
  - Daemon is not running: UI displays "Stopped" status and instructions on starting the daemon via CLI.
- **Error Scenarios**:
  - Fetch fails: UI raises a toast error warning.
- **Postconditions**: User sees accurate daemon health and log records.
- **Affected Files**:
  - `internal/uiserver/daemon_dream_handler.go`
  - `internal/ui/src/api/daemon.ts`
  - `internal/ui/src/components/daemon/DaemonDashboard.tsx`

### UC-02: Stop Daemon Process
- **Actor**: User (via UI Dashboard)
- **Preconditions**: Daemon process is active and running.
- **Main Flow**:
  1. User clicks the "Stop Daemon" button.
  2. Browser requests confirmation. User confirms.
  3. UI requests `POST /api/daemon/stop`.
  4. Go backend parses process PID, terminates the daemon process, and returns a success response.
  5. UI displays a success toast and refreshes status to "Stopped".
- **Affected Files**:
  - `internal/uiserver/daemon_dream_handler.go`
  - `internal/ui/src/api/daemon.ts`
  - `internal/ui/src/components/daemon/DaemonDashboard.tsx`

### UC-03: View Dream Status & Reports
- **Actor**: User (via UI Dashboard)
- **Preconditions**: Unified server is running.
- **Main Flow**:
  1. User navigates to "Dream Module" in the sidebar.
  2. UI fetches `GET /api/dream/status`, `GET /api/dream/reports`, and `GET /api/dream/subjects`.
  3. Go backend reads the project lockfile configuration and scans `.graphit/dream/` folder.
  4. UI renders stats (Enabled state, timeouts, total reports) and defaults to the "Dream Reports" list.
  5. User clicks on a report in the list to view its full compiled Markdown content.
- **Affected Files**:
  - `internal/uiserver/daemon_dream_handler.go`
  - `internal/ui/src/api/dream.ts`
  - `internal/ui/src/components/dream/DreamDashboard.tsx`

### UC-04: Manage Dream Subjects Queue
- **Actor**: User (via UI Dashboard)
- **Preconditions**: User is on the "Dream Subjects" tab.
- **Main Flow**:
  1. User clicks "Queue Dream Subject".
  2. UI displays subject creation form (Title and optional instructions).
  3. User submits. UI sends `POST /api/dream/subject`.
  4. Go backend saves the new subject file in the workspace `.graphit/dream/subjects/`.
  5. UI triggers a toast success banner and refreshes the subjects queue.
- **Alternative Flows**:
  - User deletes a subject: User clicks the trash icon, UI sends `DELETE /api/dream/subject/{slug}`, Go backend removes the subject file, and the list updates.
- **Affected Files**:
  - `internal/uiserver/daemon_dream_handler.go`
  - `internal/ui/src/api/dream.ts`
  - `internal/ui/src/components/dream/DreamDashboard.tsx`

## Test Cases & Acceptance Criteria

### Feature: Daemon Monitoring & Control
Ref: UC-01, UC-02

#### Scenario: View running daemon process
```gherkin
Given the background daemon process is active
  And the daemon log file contains recent activities
When the user opens the Daemon Status dashboard
Then the UI should show the state as "Running"
  And the console log view should render the recent lines of the daemon log file
```

#### Scenario: Stop the background daemon
```gherkin
Given the daemon process is active
When the user triggers the "Stop Daemon" action
  And confirms the dialog prompt
Then the process should receive a termination signal
  And a success toast notification should be displayed
  And the status badge should update to "Stopped"
```

### Feature: Dream Module Management
Ref: UC-03, UC-04

#### Scenario: Queue a new dream subject
```gherkin
Given the user is on the Dream Subjects tab
When the user submits a new subject with title "Add auth unit tests"
Then the backend should save the subject configuration file
  And the subject should appear in the pending list
  And a success toast should be shown
```

#### Scenario: Browse and view a sleep session report
```gherkin
Given a sleep session has run and generated a markdown report
When the user clicks the report title in the reports tab
Then the full report content should render in the Markdown preview pane
```

## Files Changed
| File | Change | Reason |
|---|---|---|
| `internal/uiserver/daemon_dream_handler.go` | Created | Go HTTP endpoints for daemon/dream actions |
| `internal/uiserver/daemon_dream_handler_test.go` | Created | Unit tests for daemon/dream handler endpoints |
| `internal/uiserver/unified_server.go` | Modified | Registered new daemon/dream routes |
| `internal/ui/src/api/daemon.ts` | Created | TS client for daemon status/stop |
| `internal/ui/src/api/dream.ts` | Created | TS client for dream status/subjects/reports |
| `internal/ui/src/components/daemon/DaemonDashboard.tsx` | Created | Daemon Status page |
| `internal/ui/src/components/dream/DreamDashboard.tsx` | Created | Dream Module dashboard and management panel |
| `internal/ui/src/components/layout/Sidebar.tsx` | Modified | Add sidebar menu navigation links |
| `internal/ui/src/App.tsx` | Modified | Configured UI routing for new pages |

## Trade-offs & Decisions
- **Microtask Deferral**: Handled synchronous `setState` in `useEffect` using `setTimeout` with a `0` delay. This fulfills the `react-hooks/set-state-in-effect` ESLint rule without causing runtime page jitter.
- **useCallback Memoization**: Wrapped data loading functions (`fetchData`, `fetchStatus`) in `useCallback` to allow clean inclusion in the `useEffect` dependencies list without trigger loops.

## Technical Debt
- None created. All linting warnings and TypeScript checks are clean and compiled successfully.

## Progress Log

### 2026-06-02
- Implemented Go endpoints for daemon and dream status, stop, reports, and subjects.
- Created React dashboards for daemon status/logs and dream module reports/subjects.
- Added API modules, routing, and menu sidebar links.

### 2026-06-03
- Refactored `DaemonDashboard.tsx` and `DreamDashboard.tsx` to fix ESLint rules regarding synchronous `setState` within effects.
- Cleaned unused imports and updated parameter ordering for `showToast`.
- Fixed TypeError `Cannot read properties of null (reading 'length')` inside `DreamDashboard.tsx` by ensuring the Go API handlers (`handleDreamReports` and `handleDreamSubjects`) never return `nil` arrays and the React code falls back to `[]`.
- Restyled page containers, headers, and refresh buttons in both Daemon and Dream views to match the premium aesthetics (spacing, glass-panel styles, and hover transition scales) of other screens like `RegistryPage.tsx`.
- Renamed system sidebar links to "Daemon" and "Dream" to match the clean nomenclature used in the rest of the application.
- Exported `wikiLinkFriendlyName` to shared `internal/ui/src/lib/utils.ts` and updated components to fix the TS compilation error and HMR fast refresh warning.
- Standardized item paddings in lists and aligned subject creation form modal structure to match `SubmitModal` design system.
- Centered the icon in the refresh button using flexbox classes.
- Verified system build and tests using `make ci`.
