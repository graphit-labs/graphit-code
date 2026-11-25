import { postAgentStream, type StreamOptions } from "./agentStream";
import type { AstKey } from './nativeAst';
import type { NativeInteger } from './nativeJson';



export interface GraphNode {
  id: string;
  name: string;
  label: string;
  type: string;
  file?: string;

  line?: NativeInteger;
  nativeIdentity?: { table: number; row: string };
  nativeKey?: AstKey;
  properties?: Record<string, unknown>;
}

export interface GraphEdge {
  id?: string;
  uid?: string;
  source: string;
  target: string;
  type: string;
  properties?: Record<string, unknown>;
}




export interface SchemaNodeStat {
  label: string;
  count: number | string | bigint;
}

export interface SchemaEdgeStat {
  type: string;
  count: number | string | bigint;
}

export interface SchemaLangGroup {
  lang: string;
  count: number | string | bigint;
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




export const astApi = {
  generateCypher: (prompt: string, context?: string, projectDir?: string, options: StreamOptions = {}, projectId?: string) =>
    postAgentStream<{
      cypher: string;
      explanation?: string;
      agent_session_id?: string;
      agent_cli?: string;
    }>("/api/generate-cypher", {
      query: prompt,
      context,
      project_dir: projectDir,
      project_id: projectId,
    }, options),
};
