import { postAgentStream, type StreamOptions } from "./agentStream";
import { api } from "./client";

export interface Context {
  id: string;
  name: string;
  type: "project" | "import";
  database?: string;
  node_count?: number;
  edge_count?: number;
  path?: string;
  imported_at?: string;
  db_path?: string;
}

export interface ContextsResponse {
  contexts: Context[];
  project_root: string;
  project_name: string;
}

export interface GraphNode {
  id: string;
  name: string;
  label: string;
  type: string;
  file?: string;

  line?: number;
  properties?: Record<string, unknown>;
}

export interface GraphEdge {
  source: string;
  target: string;
  type: string;
}

export interface TabularResult {
  columns: string[];
  rows: unknown[][];
}

export interface GraphResponse {
  nodes: GraphNode[];
  links: GraphEdge[];
  files: string[];
  fileContents: Record<string, string>;
  tabular?: TabularResult;
}

export interface SchemaNodeStat {
  label: string;
  count: number;
}

export interface SchemaEdgeStat {
  type: string;
  count: number;
}

export interface SchemaLangGroup {
  lang: string;
  count: number;
  labels: SchemaNodeStat[];
}

export interface SchemaResponse {
  node_types?: {
    label: string;
    identity_property: string;
    properties: string[];
  }[];
  relationship_endpoints?: { type: string; from: string; to: string }[];
  nodes: SchemaNodeStat[];
  edges: SchemaEdgeStat[];
  langs?: SchemaLangGroup[];
  node_labels: string[];
  edge_types: string[];
  backend: string;
}

export interface QueryResult {
  records: Record<string, unknown>[];
  stats: {
    nodes_created: number;
    relationships_created: number;
    properties_set: number;
  };
}

export interface StatusResponse {
  status: string;
  backend: string;
  connected: boolean;
  port: number;
  repo: string;
}

export interface CodeSearchResult {
  Type: string;
  Name: string;
  Path: string;
  Line: number;
  Source?: string;
  Docstring?: string;
  IsDepend?: boolean;
  SearchType?: string;
  RelevanceScore?: number;
  Distance?: number;
}
export const astApi = {
  search: (query: string, context?: string, projectDir?: string) => {
    const qs = new URLSearchParams({ q: query, top: "20" });
    if (context) qs.set("context", context);
    if (projectDir) qs.set("project_dir", projectDir);
    return api.get<CodeSearchResult[]>(`/api/search?${qs}`);
  },
  getContexts: (projectDir?: string) => {
    const params = new URLSearchParams();
    if (projectDir) params.set("project_dir", projectDir);
    const qs = params.toString();
    return api.get<ContextsResponse>(`/api/contexts${qs ? `?${qs}` : ""}`);
  },
  getSchema: (context?: string, projectDir?: string) => {
    const qs = new URLSearchParams();
    if (context) qs.set("context", context);
    if (projectDir) qs.set("project_dir", projectDir);
    return api.get<SchemaResponse>(`/api/schema?${qs}`);
  },
  getGraph: (params: {
    context?: string;
    cypher_query?: string;
    repo_path?: string;
    project_dir?: string;
  }) => {
    const qs = new URLSearchParams();
    if (params.context) qs.set("context", params.context);
    if (params.cypher_query) qs.set("cypher_query", params.cypher_query);
    if (params.repo_path) qs.set("repo_path", params.repo_path);
    if (params.project_dir) qs.set("project_dir", params.project_dir);
    return api.get<GraphResponse>(`/api/graph?${qs}`);
  },
  getFile: (path: string, context?: string, projectDir?: string) => {
    const qs = new URLSearchParams({ path });
    if (context) qs.set("context", context);
    if (projectDir) qs.set("project_dir", projectDir);
    return api.get<{ content: string; source: string }>(`/api/file?${qs}`);
  },
  query: (cypher: string, context?: string, projectDir?: string) =>
    api.post<QueryResult>("/api/query", {
      cypher,
      context,
      project_dir: projectDir,
    }),
  generateCypher: (prompt: string, context?: string, projectDir?: string, options: StreamOptions = {}) =>
    postAgentStream<{
      cypher: string;
      explanation?: string;
      agent_session_id?: string;
      agent_cli?: string;
    }>("/api/generate-cypher", {
      query: prompt,
      context,
      project_dir: projectDir,
    }, options),
  getStatus: () => api.get<StatusResponse>("/api/status"),
  deleteContext: (name: string, projectDir?: string) => {
    const params = new URLSearchParams();
    if (projectDir) params.set("project_dir", projectDir);
    return api.delete<{ status: string; context: string }>(
      `/api/context/${encodeURIComponent(name)}${params.size ? "?" + params.toString() : ""}`,
    );
  },
};
