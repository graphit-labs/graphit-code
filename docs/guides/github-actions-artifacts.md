# Publish native artifacts from CI

This guide publishes an already maintained PROJECT's current native AST/Knowledge snapshots.
Publication is explicit; `graphit sync` does not index, wait for full coverage, clone an ancestor
or publish a Hub release. Current commands are `hub publish-native`, not `hub submit`.

## Prerequisites and identity

Install the intended complete Graphit release/package and its paired native assets. The producer
checkout must retain its actual `.graphit/project.json` and selected version, and its daemon must
have published the data you intend to share. Native export/publication captures a coherent snapshot;
it is not a universal signal that every source or embedding job finished. Decide whether the
release represents progressive coverage or requires a separately verified complete snapshot.

For Broker-managed S3, use a valid Broker account/profile with current producer PROJECT permission,
Hub publication rights and storage discovery. The Broker issues restricted STSv3 credentials in
memory; data uploads directly to S3. A Local provider's static Broker AI key does **not** magically
give that provider Broker-managed S3 storage: standalone Local storage needs its own authorized
configuration. Do not distribute permanent Broker S3 keys to runners.

The current CLI has native browser/device login and rotating `refresh`; it does not have a generic
`--access-token`/ID-token importer or a client-credentials grant for a fresh headless runner.
`--non-interactive login` still requires completing the actual public authorization flow. Do not
invent unattended login by writing Auth JSON or storing a copied global directory in Git/artifacts.
Use an appropriately protected maintained runner/service with an existing admitted identity, or
implement a separately authorized workload-auth integration before promising ephemeral unattended
Broker publication. [Authentication](authentication.md) defines the supported flows.

## Example: prepared self-hosted publisher

The following is a template for a protected runner whose private installation/provider/profile
and maintained source snapshot are already configured. Values are project variables; credentials
are not echoed or written by these steps. Select actual IDs, provider and artifact versions.

```yaml
name: Publish prepared Graphit context
on: workflow_dispatch
jobs:
  publish:
    runs-on: self-hosted
    env:
      GRAPHIT_GLOBAL_DIR: ${{ vars.GRAPHIT_PUBLISHER_GLOBAL_DIR }}
      HUB_PROVIDER: ${{ vars.GRAPHIT_HUB_PROVIDER }}
      AST_ID: ${{ vars.GRAPHIT_AST_ARTIFACT_ID }}
      KNOWLEDGE_ID: ${{ vars.GRAPHIT_KNOWLEDGE_ARTIFACT_ID }}
      ARTIFACT_VERSION: ${{ github.sha }}
    steps:
      - uses: actions/checkout@v4
      - name: Verify the prepared runtime
        run: |
          test -n "$GRAPHIT_GLOBAL_DIR"
          test -f .graphit/project.json
          graphit daemon status
          graphit ast schema
          graphit knowledge search configuration --mode fts --rerank=false
      - name: Publish the selected native snapshots
        run: |
          graphit --non-interactive hub publish-native --kind ast \
            --provider "$HUB_PROVIDER" --project-dir . \
            --id "$AST_ID" --version "$ARTIFACT_VERSION" --name 'Project code'
          graphit --non-interactive hub publish-native --kind knowledge \
            --provider "$HUB_PROVIDER" --project-dir . \
            --id "$KNOWLEDGE_ID" --version "$ARTIFACT_VERSION" --name 'Project documentation'
```

Readiness above proves a readable publication, **not complete source/semantic coverage**. The
prepared-runner prerequisite includes current checkout selection and the desired coverage; do not
publish a different checkout's stale snapshot under this SHA. `projects maintain-checkout` updates
selection, but its acknowledgement alone does not wait for all indexing. Gate the intended producer
state before this template where full coverage is required.

AST and Knowledge publications are independent. The first can succeed if the second fails; inspect
actual outcomes before retry. A lost response is not proof of rollback. Artifact version is the
literal version supplied here, separate from project branch/version and SCIP compatibility alias.
Use the existing Hub conflict/replacement rules rather than an invented idempotent-overwrite promise.

## Models, scopes and data handling

FTS/native text/graph publication need no completion model or rerank. If semantic vectors are
required, configure the actual embedding service and wait for the intended compatible coverage.
A Broker provider routes enabled embedding/rerank only through Broker; rerank stays opt-in and is
not required for export. Remote inference requires no local GGUF/ONNX model download.

Never upload or cache `auth.json`, private runtime keys, credentials, cookies or the installation's
whole global directory as a build artifact. Native packages contain their admitted snapshots and
public artifact metadata. [Package formats](../development/native-packages.md),
[Hub lifecycle](../specs/hub_collaboration.md) and [S3](s3-and-ui-network.md) define integrity,
authorization and actual storage boundaries. A configured workflow is not a remote execution PASS.
