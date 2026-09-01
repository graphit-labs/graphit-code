package config

// AgentModule is the module name gating every feature that needs a coding-agent CLI on the
// machine — not an API key, not an embedding model, but an installed and authenticated agent
// binary that the framework shells out to.
//
// Disable it with `modules.agent false`, or with the environment variable ConfigEnvVar derives
// for that key. Like every other module it is ON by default; it is not in OptInModules.
//
// What it covers, and why these four and nothing else: each one reaches
// ai.NewClientFromConfig, which only ever returns a CLI client from exec.LookPath. There is no
// HTTP fallback behind it, so with no agent binary present the feature cannot degrade — it can
// only fail.
//
//   - POST /api/generate-cypher — natural-language querying in the AST explorer
//   - POST /api/wiki/ai-search  — AI search in the knowledge explorer AND the memory explorer,
//     which are one component over one route
//   - /api/live/*               — live search, which additionally prepares an ephemeral project
//     per session and streams an agent's turns over SSE
//
// What it deliberately does NOT cover: everything that runs on local ONNX embeddings or on the
// graph alone. `GET /api/search` (BM25 + vector hybrid), `GET /api/wiki/search` (BM25), and every
// Cypher, graph, complexity and dead-code route keep working with the module off. A container
// with no agent CLI is a fully useful Hub and explorer; it just cannot answer in prose.
const AgentModule = "agent"

// AgentFeaturesEnabled reports whether the agent-CLI-dependent features may be offered.
//
// It answers from configuration only, and deliberately does not probe for the binary: a machine
// where the CLI is temporarily missing should say so at the point of use, with the message that
// names the fix, rather than have the UI silently reshape itself. This flag is the operator
// saying "there is no agent here and there will not be one" — which is exactly the situation a
// container image is in.
func AgentFeaturesEnabled(inlineCfg, projectCfg ConfigMap) bool {
	return !IsModuleDisabled(AgentModule, inlineCfg, projectCfg)
}
