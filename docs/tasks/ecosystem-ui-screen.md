---
title: "Ecosystem UI Screen"
status: completed
created: 2026-06-18
---

# Ecosystem UI Screen

## Summary

Added an **Ecosystem** management page in the System menu of the UI. This screen allows developers to view and manage all local project directories registered in the global lock manager (`global.lock.json`).

## Changes

### Go Backend

#### `internal/hub/global_lock.go`
- Added `Name`, `Description`, `Cluster`, and `RegisteredAt` fields to the `ActiveProject` struct.
- Updated the `ListActiveProjects` method to fully populate these fields from the `InstanceEntry` objects stored in the global lock.

#### `internal/hub/ui_server.go`
- Extended the `handleGlobalProjects` API handler (`GET /api/global-projects`) to output all project metadata fields.
- Registered three new API endpoints for managing the project cluster and registration states:
  - `POST /api/cluster/set` -> sets a cluster label key/value pair on a project instance
  - `POST /api/cluster/unset` -> unsets a cluster label on a project instance
  - `POST /api/project/unregister` -> unregisters a project from the managed active projects list
- Implemented `handleSetCluster`, `handleUnsetCluster`, and `handleUnregisterProject` handler methods.

---

### React Frontend

#### `internal/ui/src/api/hub.ts`
- Extended the `GlobalProject` TypeScript interface to include `description`, `registered_at`, and `cluster` fields.
- Added TypeScript wrappers for the new endpoints: `setClusterLabel`, `unsetClusterLabel`, and `unregisterProject`.

#### `internal/ui/src/components/system/EcosystemDashboard.tsx`
- Created a dashboard component that lists all registered local projects.
- Displays project details: Name, Directory, Registered date, Description, and Cluster Labels.
- Added actions:
  - **Switch Project**: Activates the selected project in Zustand state.
  - **Cluster Tag Management**: Allows users to add key-value tags inline, or remove them with a single click.
  - **Unregister Project**: Prompts for confirmation and calls the unregister endpoint.
- Features search/filtering, copy-to-clipboard for paths, beautiful glassmorphic elements, and micro-animations.

#### `internal/ui/src/App.tsx`
- Lazy-loaded `EcosystemDashboard` component.
- Registered the `/system/ecosystem` route inside the main `AppShell` routes list.

#### `internal/ui/src/components/layout/Sidebar.tsx`
- Imported the `Globe` icon.
- Appended the "Ecosystem" link under the "System" navigation section pointing to `/system/ecosystem`.

---

## Verification

- Ran Go tests in `internal/hub/...`:
  - `go test -v ./internal/hub/...` passed.
- Compiled the UI code:
  - `npm run build` completed successfully without any compilation or TypeScript errors.
