const API_BASE = '';

export async function fetchJson<T>(path: string): Promise<T> {
  const response = await fetch(API_BASE + path);
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }
  return response.json();
}

export async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(API_BASE + path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }
  return response.json();
}

export interface GraphNode {
  id: string;
  name: string;
  type: string;
  file?: string;
  line?: number;
  complexity?: number;
  labels?: string[];
}

export interface GraphEdge {
  from: string;
  to: string;
  edge_type?: string;
}

export interface GraphStats {
  node_count: number;
  edge_count: number;
  function_count: number;
  class_count: number;
  avg_complexity: number;
}

export interface Community {
  id: number;
  member_count: number;
}

export interface CentralityData {
  nodes: Array<{
    name: string;
    type: string;
    in_degree: number;
    out_degree: number;
    betweenness: number;
    pagerank: number;
    file?: string;
  }>;
}

export interface ComplexFunction {
  name: string;
  complexity: number;
  file: string;
  line?: number;
}

export const api = {
  async getNodes(params?: { limit?: number; node_type?: string }) {
    const query = new URLSearchParams();
    if (params?.limit) query.set('limit', params.limit.toString());
    if (params?.node_type) query.set('node_type', params.node_type);
    return fetchJson<{ nodes: GraphNode[] }>(`/api/graph/nodes?${query}`);
  },

  async getEdges(limit = 5000) {
    return fetchJson<{ edges: GraphEdge[] }>(`/api/graph/edges?limit=${limit}`);
  },

  async getStats() {
    return fetchJson<GraphStats>('/api/graph/stats');
  },

  async getCommunities() {
    return fetchJson<{ communities: Community[] }>('/api/communities');
  },

  async getCentrality() {
    return fetchJson<CentralityData>('/api/centrality');
  },

  async getTopComplex() {
    return fetchJson<{ functions: ComplexFunction[] }>('/api/top-complex');
  },

  async searchNodes(query: string, limit = 20) {
    return fetchJson<{ nodes: GraphNode[] }>(`/api/graph/search?q=${encodeURIComponent(query)}&limit=${limit}`);
  },

  async getSymbol(name: string) {
    return fetchJson<any>(`/api/symbol/${encodeURIComponent(name)}`);
  },
};
