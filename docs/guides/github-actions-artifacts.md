# Publishing Graphit artifacts from GitHub Actions

Artifact publishing in CI requires an S3-enabled Graphit Broker and a remote embedding
service. The runner must never receive S3 keys or topology. Setup owns runtime preferences, ensures
the default `local` provider, and provisions models whose manifest selects `fetch_policy: setup`;
provider configuration owns custom AI routing and local ONNX execution settings, and login happens
in an ephemeral global directory.

Create these GitHub secrets/variables:

- `GRAPHIT_BROKER_URL` and, when using a static service identity, `GRAPHIT_BROKER_KEY`;
- `GRAPHIT_AST_ARTIFACT_ID` and `GRAPHIT_KNOWLEDGE_ARTIFACT_ID` for the two stable Hub IDs;
- either an existing non-interactive OIDC access/ID/refresh token set, or a workflow step that
  obtains them for the broker audience;
- a broker deployment that advertises the required embedding service; the runner never receives a
  direct embedding or rerank provider key.

Static broker-key example:

```yaml
- name: Configure Graphit runtime
  env:
    GRAPHIT_GLOBAL_DIR: ${{ runner.temp }}/graphit-global
  run: |
    graphit --non-interactive setup \
      --anonymize-events=false --agent=codex --cli=codex

- name: Configure broker provider and login
  env:
    GRAPHIT_GLOBAL_DIR: ${{ runner.temp }}/graphit-global
    BROKER_URL: ${{ vars.GRAPHIT_BROKER_URL }}
    BROKER_KEY: ${{ secrets.GRAPHIT_BROKER_KEY }}
  run: |
    graphit --non-interactive provider add ci --type local \
      --broker-endpoint "$BROKER_URL" \
      --embedding-mode broker \
      --rerank-mode broker
    graphit --non-interactive login \
      --profile publisher --provider ci \
      --username ci-publisher --team release-engineering \
      --broker-key "$BROKER_KEY"
```

Both broker modes are required by provider validation. Reranking is still inactive by default
because `search.rerank=false`, so declaring the broker rerank backend does not add a CI request.

OIDC example after a trusted step acquires tokens for the configured broker audience:

```yaml
- name: Configure OIDC broker provider
  env:
    GRAPHIT_GLOBAL_DIR: ${{ runner.temp }}/graphit-global
  run: |
    graphit --non-interactive provider add ci-oidc --type oidc \
      --issuer '${{ vars.GRAPHIT_OIDC_ISSUER }}' \
      --client-id '${{ vars.GRAPHIT_OIDC_CLIENT_ID }}' \
      --username-claim preferred_username \
      --organization-claim organization \
      --teams-claim groups \
      --broker-endpoint '${{ vars.GRAPHIT_BROKER_URL }}' \
      --broker-audience graphit-services \
      --embedding-mode broker --rerank-mode broker

- name: Activate publishing identity
  env:
    GRAPHIT_GLOBAL_DIR: ${{ runner.temp }}/graphit-global
    OIDC_ACCESS_TOKEN: ${{ secrets.GRAPHIT_OIDC_ACCESS_TOKEN }}
    OIDC_ID_TOKEN: ${{ secrets.GRAPHIT_OIDC_ID_TOKEN }}
    OIDC_REFRESH_TOKEN: ${{ secrets.GRAPHIT_OIDC_REFRESH_TOKEN }}
  run: |
    graphit --non-interactive login --profile publisher --provider ci-oidc \
      --access-token "$OIDC_ACCESS_TOKEN" \
      --id-token "$OIDC_ID_TOKEN" \
      --refresh-token "$OIDC_REFRESH_TOKEN"
```

Discovery must advertise both the broker embedding capability and `graphit-s3-credentials-v2`
before publishing. Graphit requests a short-lived STS session for the publishing project whose
policy is derived from the current principal's grants, then uploads directly to S3. The Broker owns bucket, region, endpoint,
prefix, role and its permanent AWS identity; the runner receives only the temporary session.

For a repository that already commits `graphit.lock.json`, use the following job steps with one of
the login sequences above. These are composable steps, not a standalone workflow: install a Graphit
binary built from the intended source revision before runtime setup, and provide a working broker
identity. Run this job on a branch push at the committed `HEAD`. Full Git history lets sync choose
the nearest published ancestor. `graphit sync` hydrates that base and builds the local AST and
Knowledge overlay; it does **not** publish. The two `hub submit` calls publish the current Git
commit to the branch, applying changed LanceDB rows and changed non-Lance objects only.

```yaml
- uses: actions/checkout@v4
  with:
    fetch-depth: 0

# Insert the runtime setup and one broker login sequence shown above here.

- name: Index and publish the branch commit
  env:
    GRAPHIT_GLOBAL_DIR: ${{ runner.temp }}/graphit-global
    GRAPHIT_GIT_BASE_BRANCH: ${{ github.ref_name }}
    AST_ID: ${{ vars.GRAPHIT_AST_ARTIFACT_ID }}
    KNOWLEDGE_ID: ${{ vars.GRAPHIT_KNOWLEDGE_ARTIFACT_ID }}
  run: |
    test "${GITHUB_REF_TYPE}" = branch
    test -f graphit.lock.json
    test -n "$AST_ID" && test -n "$KNOWLEDGE_ID"
    graphit sync --no-background
    test -z "$(git status --porcelain)"
    graphit hub submit "$AST_ID" . --type ast --version "branch/$GRAPHIT_GIT_BASE_BRANCH"
    graphit hub submit "$KNOWLEDGE_ID" . --type knowledge --version "branch/$GRAPHIT_GIT_BASE_BRANCH"
```

The checkout must remain clean after indexing; commit intended source and lockfile changes before
running CI. Git-backed AST and Knowledge branch publication records the commit in Hub history
without modifying the project lockfile, so both artifacts can publish from the same commit. The
diff is logical: an unchanged LanceDB row or file is not rewritten, but a changed non-Lance file
is uploaded as a whole object and LanceDB may rewrite affected fragments. Repeating the same
commit with no changes adds no LanceDB table version. A web MCP client can use `graphit_hub_submit`
with the same `branch/<name>` version if the project and indexed artifact paths exist on the MCP
server host; a remote client cannot upload its own local path through this tool.

Disable unused background modules for an ephemeral publisher. Never cache or upload the runner's
global auth directory, and never log tokens, broker keys, or temporary S3 credentials. Broker and
direct OIDC STS values live only in runner memory and are reacquired after a new process starts.
