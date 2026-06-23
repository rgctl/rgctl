import { useEffect, useRef, useState } from 'react';
import * as d3 from 'd3';
import { api, GraphNode, GraphEdge } from '../utils/api';

const TYPE_COLORS: Record<string, string> = {
  Function: '#58a6ff',
  Class: '#bc8cff',
  Struct: '#79c0ff',
  File: '#8b949e',
  Module: '#3fb950',
  ConfigKey: '#d29922',
  default: '#6e7681',
};

export function GraphBrowser() {
  const svgRef = useRef<SVGSVGElement>(null);
  const [nodes, setNodes] = useState<GraphNode[]>([]);
  const [edges, setEdges] = useState<GraphEdge[]>([]);
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [stats, setStats] = useState<any>(null);
  const [typeFilter, setTypeFilter] = useState('');
  const [searchQuery, setSearchQuery] = useState('');
  const [loading, setLoading] = useState(false);

  const loadGraph = async () => {
    setLoading(true);
    try {
      const params: any = { limit: 300 };
      if (typeFilter) params.node_type = typeFilter;

      const [nodeData, edgeData, statsData] = await Promise.all([
        api.getNodes(params),
        api.getEdges(5000),
        api.getStats(),
      ]);

      const nodeIds = new Set(nodeData.nodes.map((n) => n.id));
      const filteredEdges = edgeData.edges.filter(
        (e) => nodeIds.has(e.from) && nodeIds.has(e.to)
      );

      setNodes(nodeData.nodes);
      setEdges(filteredEdges);
      setStats(statsData);
    } catch (error) {
      console.error('Error loading graph:', error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadGraph();
  }, [typeFilter]);

  useEffect(() => {
    if (!svgRef.current || nodes.length === 0) return;

    const svg = d3.select(svgRef.current);
    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    svg.selectAll('*').remove();

    svg.attr('viewBox', [0, 0, width, height]);

    const g = svg.append('g');

    svg.call(
      d3.zoom<SVGSVGElement, unknown>()
        .scaleExtent([0.1, 4])
        .on('zoom', (event) => {
          g.attr('transform', event.transform.toString());
        })
    );

    const simulation = d3
      .forceSimulation(nodes as any)
      .force(
        'link',
        d3
          .forceLink(edges.map((e) => ({ source: e.from, target: e.to })))
          .id((d: any) => d.id)
          .distance(60)
      )
      .force('charge', d3.forceManyBody().strength(-200))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(20));

    const link = g
      .selectAll('.link')
      .data(edges)
      .join('line')
      .attr('class', 'link')
      .attr('stroke', '#30363d')
      .attr('stroke-width', 1.5);

    const node = g
      .selectAll('.node')
      .data(nodes)
      .join('g')
      .attr('class', 'node')
      .style('cursor', 'pointer')
      .on('click', (_, d) => setSelectedNode(d))
      .call(
        d3.drag<any, any>()
          .on('start', (event, d: any) => {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            d.fx = d.x;
            d.fy = d.y;
          })
          .on('drag', (event, d: any) => {
            d.fx = event.x;
            d.fy = event.y;
          })
          .on('end', (event, d: any) => {
            if (!event.active) simulation.alphaTarget(0);
            d.fx = null;
            d.fy = null;
          })
      );

    node
      .append('circle')
      .attr('r', 8)
      .attr('fill', (d) => TYPE_COLORS[d.type] || TYPE_COLORS.default)
      .attr('stroke', '#30363d')
      .attr('stroke-width', 2);

    node
      .append('text')
      .attr('x', 12)
      .attr('y', 4)
      .attr('fill', '#e6edf3')
      .attr('font-size', 10)
      .style('pointer-events', 'none')
      .style('user-select', 'none')
      .text((d) => (d.name.length > 18 ? d.name.slice(0, 16) + '…' : d.name));

    simulation.on('tick', () => {
      link
        .attr('x1', (d: any) => d.source.x)
        .attr('y1', (d: any) => d.source.y)
        .attr('x2', (d: any) => d.target.x)
        .attr('y2', (d: any) => d.target.y);

      node.attr('transform', (d: any) => `translate(${d.x},${d.y})`);
    });

    // Live search filter
    if (searchQuery) {
      const q = searchQuery.toLowerCase();
      node.style('opacity', (d) =>
        d.name.toLowerCase().includes(q) ? 1 : 0.15
      );
      link.style('opacity', 0.1);
    } else {
      node.style('opacity', 1);
      link.style('opacity', 1);
    }

    return () => {
      simulation.stop();
    };
  }, [nodes, edges, searchQuery]);

  return (
    <div className="grid grid-cols-[280px_1fr_300px] h-full">
      {/* Left Sidebar - Stats */}
      <aside className="bg-[#161b22] border-r border-[#30363d] overflow-y-auto p-3">
        <h2 className="text-sm font-medium mb-2">Statistics</h2>
        {stats ? (
          <div className="space-y-2">
            <div className="flex justify-between py-2 border-b border-[#21262d] text-sm">
              <span className="text-[#8b949e]">Nodes</span>
              <strong className="text-[#58a6ff]">{stats.node_count}</strong>
            </div>
            <div className="flex justify-between py-2 border-b border-[#21262d] text-sm">
              <span className="text-[#8b949e]">Edges</span>
              <strong className="text-[#58a6ff]">{stats.edge_count}</strong>
            </div>
            <div className="flex justify-between py-2 border-b border-[#21262d] text-sm">
              <span className="text-[#8b949e]">Functions</span>
              <strong className="text-[#58a6ff]">{stats.function_count}</strong>
            </div>
            <div className="flex justify-between py-2 border-b border-[#21262d] text-sm">
              <span className="text-[#8b949e]">Avg Complexity</span>
              <strong className="text-[#58a6ff]">
                {(stats.avg_complexity || 0).toFixed(1)}
              </strong>
            </div>
          </div>
        ) : (
          <div className="text-[#8b949e] text-sm">Loading...</div>
        )}

        <div className="mt-4">
          <label className="block text-sm mb-2">Type Filter</label>
          <select
            value={typeFilter}
            onChange={(e) => setTypeFilter(e.target.value)}
            className="w-full bg-[#21262d] border border-[#30363d] text-[#e6edf3] px-2 py-1 rounded text-sm"
          >
            <option value="">All types</option>
            <option value="Function">Function</option>
            <option value="Class">Class</option>
            <option value="Struct">Struct</option>
            <option value="File">File</option>
            <option value="Module">Module</option>
          </select>
        </div>

        <button
          onClick={loadGraph}
          disabled={loading}
          className="mt-4 w-full bg-[#238636] text-white px-3 py-2 rounded text-sm hover:bg-[#2ea043] disabled:opacity-50"
        >
          {loading ? 'Loading...' : 'Refresh'}
        </button>
      </aside>

      {/* Center - Graph */}
      <div className="relative bg-[#0d1117]">
        <svg ref={svgRef} className="w-full h-full" />
      </div>

      {/* Right Sidebar - Node Detail */}
      <aside className="bg-[#161b22] border-l border-[#30363d] overflow-y-auto p-3">
        <input
          type="text"
          placeholder="Search nodes..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="w-full bg-[#21262d] border border-[#30363d] text-[#e6edf3] px-3 py-2 rounded text-sm mb-4"
        />

        {selectedNode ? (
          <div>
            <h3 className="text-sm font-medium mb-2 text-[#58a6ff]">
              {selectedNode.name}
            </h3>
            <p className="text-xs text-[#8b949e] mb-1">
              <strong>Type:</strong> {selectedNode.type}
            </p>
            <p className="text-xs text-[#8b949e] mb-1">
              <strong>File:</strong> {selectedNode.file || '?'}
            </p>
            <p className="text-xs text-[#8b949e] mb-1">
              <strong>Line:</strong> {selectedNode.line || '?'}
            </p>
            {selectedNode.complexity && (
              <p className="text-xs text-[#8b949e] mb-1">
                <strong>Complexity:</strong> {selectedNode.complexity}
              </p>
            )}
          </div>
        ) : (
          <p className="text-[#8b949e] text-sm">
            Select a node to view details.
          </p>
        )}
      </aside>
    </div>
  );
}
