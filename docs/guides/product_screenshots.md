# Product screenshots

The public examples show **Aster Delivery**, a fictional order-fulfillment system. Its source code,
architecture, payment retry decision, work records, people and access policies were invented in
English for this documentation. The screenshots capture the running Code and Broker applications,
not image mockups. They contain no maintainer or customer workspace data.

## What each image demonstrates

| Image | Reader question | Published surfaces |
|---|---|---|
| [Code investigation](../site/assets/code-investigation.jpg) | Who calls this function, what does it call, and where is its implementation? | README, investigation guide, website |
| [Knowledge](../site/assets/code-knowledge.jpg) | What is the contract, and which maintained source supports it? | README, website |
| [Task evidence](../site/assets/code-evidence.jpg) | What was accepted, what evidence was recorded, and which decisions are linked? | README, website |
| [Broker grants](../site/assets/broker-resource-grants.jpg) | Which audience has which capability over which projects? | Shared website; Broker README and administration guide |

The [Broker administration guide](https://github.com/graphit-labs/graphit-broker/blob/main/docs/administration.md)
also shows the identity directory. The example's completed task contains an explicitly illustrative
documentation review, not a claim that a production payment system passed tests. Newly created
Broker accounts show their actual onboarding state. Policy examples do not imply enabled upstream services.

## Recreate examples safely

1. Build the current Code UI and binary, and the current Broker binary. Use the documented
   [design system](../specs/design_system.md); do not recolor screenshots to simulate another UI.
2. Create a disposable project outside real repositories and give Code a separate
   `GRAPHIT_GLOBAL_DIR`. Bind its UI to loopback on an unused port. Keep a manifest of the temporary
   directory, runner files, process IDs or service units, and ports.
3. Create a small English codebase with meaningful relationships: checkout calls order placement;
   order placement coordinates inventory, payment and events. Index the real files. Add maintained
   architecture and retry documents, a memory decision, a session and tasks with typed references.
   Populate through supported application services or APIs, never by fabricating API responses.
4. Start Broker with a separate SQLite database, generated disposable credentials and a loopback
   address. Disable external services and use only invented projects, teams and `example.test`
   addresses. Bootstrap and finish the local login flow, then create roles, identities and grants
   through the administration API. Use the returned CSRF token and current grant ETag for writes.
5. Capture focused workflows in English. Prefer useful evidence over an exhaustive screen catalogue.
   Check desktop and narrow layouts, light/dark behavior, loading completion and readable labels.
   Capture the actual browser; do not insert data, remove errors or alter content in the image.
6. Inspect every image for names, paths, domains, credentials and unrelated browser content. Use
   descriptive alt text and captions that explain the question answered. Link site thumbnails to
   full-size images, preserve their aspect ratio, and lazy-load images below the first screen.
7. Replace all references together. Keep the Broker grants copy on the shared site byte-identical
   to the Broker documentation asset. Remove obsolete, unreferenced screenshots.
8. Stop only the recorded demo processes. Delete their project, stores, SQLite files, credentials,
   temporary runner and browser tabs. Confirm the ports are closed and the real workspace registry
   was not modified. Retain only final images and these maintenance instructions.

The September 2026 capture used isolated local stores and a real Go/TypeScript index. Its temporary
Code and Broker environments were deleted after capture. Example data is not installed with Graphit.
