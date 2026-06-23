import { useEffect, useRef, useState } from 'react';
import * as d3 from 'd3';
import { api } from '@/utils/api';
import type { GraphNode, GraphEdge } from '@/utils/api';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';

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

    // Create links array to share between simulation and rendering
    const links = edges.map((e) => ({ source: e.from, target: e.to }));

    const simulation = d3
      .forceSimulation(nodes as any)
      .force(
        'link',
        d3
          .forceLink(links)
          .id((d: any) => d.id)
          .distance(60)
      )
      .force('charge', d3.forceManyBody().strength(-200))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(20));

    const link = g
      .selectAll('.link')
      .data(links)
      .join('line')
      .attr('class', 'link')
      .attr('stroke', 'hsl(var(--border))')
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
      .attr('stroke', 'hsl(var(--border))')
      .attr('stroke-width', 2);

    node
      .append('text')
      .attr('x', 12)
      .attr('y', 4)
      .attr('fill', 'hsl(var(--foreground))')
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
      <aside className="bg-card border-r overflow-y-auto p-3">
        <h2 className="text-sm font-semibold mb-3">Statistics</h2>
        {stats ? (
          <div className="space-y-2">
            <div className="flex justify-between py-2 border-b text-sm">
              <span className="text-muted-foreground">Nodes</span>
              <strong className="text-primary">{stats.node_count}</strong>
            </div>
            <div className="flex justify-between py-2 border-b text-sm">
              <span className="text-muted-foreground">Edges</span>
              <strong className="text-primary">{stats.edge_count}</strong>
            </div>
            <div className="flex justify-between py-2 border-b text-sm">
              <span className="text-muted-foreground">Functions</span>
              <strong className="text-primary">{stats.function_count}</strong>
            </div>
            <div className="flex justify-between py-2 border-b text-sm">
              <span className="text-muted-foreground">Avg Complexity</span>
              <strong className="text-primary">
                {(stats.avg_complexity || 0).toFixed(1)}
              </strong>
            </div>
          </div>
        ) : (
          <div className="text-muted-foreground text-sm">Loading...</div>
        )}

        <div className="mt-4 space-y-3">
          <div>
            <label className="block text-sm mb-2 font-medium">Type Filter</label>
            <Select value={typeFilter} onValueChange={setTypeFilter}>
              <SelectTrigger>
                <SelectValue placeholder="All types" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="">All types</SelectItem>
                <SelectItem value="Function">Function</SelectItem>
                <SelectItem value="Class">Class</SelectItem>
                <SelectItem value="Struct">Struct</SelectItem>
                <SelectItem value="File">File</SelectItem>
                <SelectItem value="Module">Module</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <Button onClick={loadGraph} disabled={loading} className="w-full">
            {loading ? 'Loading...' : 'Refresh'}
          </Button>
        </div>
      </aside>

      {/* Center - Graph */}
      <div className="relative bg-background">
        <svg ref={svgRef} className="w-full h-full" />
      </div>

      {/* Right Sidebar - Node Detail */}
      <aside className="bg-card border-l overflow-y-auto p-3">
        <Input
          type="text"
          placeholder="Search nodes..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="mb-4"
        />

        {selectedNode ? (
          <Card>
            <CardHeader className="p-4 pb-3">
              <CardTitle className="text-sm">{selectedNode.name}</CardTitle>
            </CardHeader>
            <CardContent className="p-4 pt-0 space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">Type:</span>
                <Badge variant="secondary">{selectedNode.type}</Badge>
              </div>
              <p className="text-xs text-muted-foreground">
                <strong>File:</strong> {selectedNode.file || '?'}
              </p>
              <p className="text-xs text-muted-foreground">
                <strong>Line:</strong> {selectedNode.line || '?'}
              </p>
              {selectedNode.complexity && (
                <p className="text-xs text-muted-foreground">
                  <strong>Complexity:</strong> {selectedNode.complexity}
                </p>
              )}
            </CardContent>
          </Card>
        ) : (
          <p className="text-muted-foreground text-sm">
            Select a node to view details.
          </p>
        )}
      </aside>
    </div>
  );
}
