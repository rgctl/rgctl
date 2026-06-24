import { useEffect, useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { ChartContainer, ChartTooltip, ChartTooltipContent } from '@/components/ui/chart';
import { BarChart, Bar, XAxis, YAxis, CartesianGrid, ResponsiveContainer, PieChart, Pie, Cell } from 'recharts';
import { api } from '@/utils/api';

const TYPE_COLORS = ['var(--chart-1)', 'var(--chart-2)', 'var(--chart-3)', 'var(--chart-4)', 'var(--chart-5)'];

export function Dashboard() {
  const [stats, setStats] = useState<any>(null);
  const [communities, setCommunities] = useState<any[]>([]);
  const [centrality, setCentrality] = useState<any[]>([]);
  const [topComplex, setTopComplex] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadDashboard();
  }, []);

  const loadDashboard = async () => {
    setLoading(true);
    try {
      // Load stats and communities (these exist)
      const [statsData, communitiesData] = await Promise.all([
        api.getStats(),
        api.getCommunities(),
      ]);

      setStats(statsData);
      setCommunities(communitiesData.communities || []);

      // Try to load optional endpoints (may not exist yet)
      try {
        const centralityData = await api.getCentrality();
        setCentrality(centralityData.nodes || []);
      } catch (e) {
        console.log('Centrality endpoint not available');
      }

      try {
        const complexData = await api.getTopComplex();
        setTopComplex(complexData.functions || []);
      } catch (e) {
        console.log('Top-complex endpoint not available');
      }
    } catch (error) {
      console.error('Error loading dashboard:', error);
    } finally {
      setLoading(false);
    }
  };

  // Prepare chart data
  const typeDistribution = stats
    ? [
        { name: 'Functions', value: stats.function_count },
        { name: 'Classes', value: stats.class_count },
        { name: 'Files', value: stats.node_count - stats.function_count - stats.class_count },
      ]
    : [];

  const complexityData = topComplex.slice(0, 10).map((f) => ({
    name: f.name.length > 15 ? f.name.slice(0, 13) + '...' : f.name,
    complexity: f.complexity,
  }));

  const communityData = communities.slice(0, 5).map((c, i) => ({
    name: `Community ${c.id}`,
    size: c.member_count,
  }));

  if (loading) {
    return (
      <div className="p-6 overflow-y-auto h-full">
        <div>
          <p className="text-muted-foreground">Loading dashboard...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="p-8 overflow-y-auto h-full bg-background">
      <div className="space-y-8">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Repository Dashboard</h1>
          <p className="text-muted-foreground mt-2">Analytics and insights for your codebase</p>
        </div>

        {/* Overview Stats */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          <Card size="sm">
            <CardHeader>
              <CardDescription>Total Nodes</CardDescription>
              <CardTitle className="text-3xl font-bold">{stats?.node_count || 0}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-xs text-muted-foreground">Graph entities</div>
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader>
              <CardDescription>Total Edges</CardDescription>
              <CardTitle className="text-3xl font-bold">{stats?.edge_count || 0}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-xs text-muted-foreground">Relationships</div>
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader>
              <CardDescription>Functions</CardDescription>
              <CardTitle className="text-3xl font-bold">{stats?.function_count || 0}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-xs text-muted-foreground">Total functions</div>
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader>
              <CardDescription>Avg Complexity</CardDescription>
              <CardTitle className="text-3xl font-bold">{(stats?.avg_complexity || 0).toFixed(1)}</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-xs text-muted-foreground">Cyclomatic complexity</div>
            </CardContent>
          </Card>
        </div>

        {/* Charts Row */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <Card>
            <CardHeader>
              <CardTitle>Complexity Distribution</CardTitle>
              <CardDescription>Top 10 most complex functions</CardDescription>
            </CardHeader>
            <CardContent>
              <ChartContainer
                config={{
                  complexity: {
                    label: 'Complexity',
                    color: 'var(--chart-1)',
                  },
                }}
                className="h-[300px]"
              >
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={complexityData}>
                    <CartesianGrid strokeDasharray="3 3" stroke="var(--border)" />
                    <XAxis dataKey="name" stroke="var(--muted-foreground)" fontSize={12} />
                    <YAxis stroke="var(--muted-foreground)" fontSize={12} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Bar dataKey="complexity" fill="var(--chart-1)" radius={[4, 4, 0, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </ChartContainer>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Node Type Distribution</CardTitle>
              <CardDescription>Breakdown by entity type</CardDescription>
            </CardHeader>
            <CardContent>
              <ChartContainer
                config={{
                  value: {
                    label: 'Count',
                  },
                }}
                className="h-[300px]"
              >
                <ResponsiveContainer width="100%" height="100%">
                  <PieChart>
                    <Pie
                      data={typeDistribution}
                      cx="50%"
                      cy="50%"
                      labelLine={false}
                      label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}
                      outerRadius={80}
                      fill="var(--chart-1)"
                      dataKey="value"
                    >
                      {typeDistribution.map((_, index) => (
                        <Cell key={`cell-${index}`} fill={TYPE_COLORS[index % TYPE_COLORS.length]} />
                      ))}
                    </Pie>
                    <ChartTooltip content={<ChartTooltipContent />} />
                  </PieChart>
                </ResponsiveContainer>
              </ChartContainer>
            </CardContent>
          </Card>
        </div>

        {/* Communities */}
        <Card>
          <CardHeader>
            <CardTitle>Code Communities</CardTitle>
            <CardDescription>Module clusters detected by graph analysis</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col gap-2">
              {communities.length > 0 ? (
                <>
                  {communities.slice(0, 10).map((c) => (
                    <div key={c.id} className="flex items-center justify-between py-2 border-b last:border-0">
                      <span className="text-sm font-medium">Community {c.id}</span>
                      <Badge variant="secondary">{c.member_count} members</Badge>
                    </div>
                  ))}
                </>
              ) : (
                <div className="text-sm text-muted-foreground">No communities detected</div>
              )}
            </div>
          </CardContent>
        </Card>

        {/* Top Complex Functions Table */}
        <Card>
          <CardHeader>
            <CardTitle>Most Complex Functions</CardTitle>
            <CardDescription>Functions with highest cyclomatic complexity</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Function</TableHead>
                  <TableHead>Complexity</TableHead>
                  <TableHead>File</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {topComplex.slice(0, 10).map((fn, i) => (
                  <TableRow key={i}>
                    <TableCell className="font-mono text-sm">{fn.name}</TableCell>
                    <TableCell>
                      <Badge variant={fn.complexity > 15 ? 'destructive' : fn.complexity > 10 ? 'default' : 'secondary'}>
                        {fn.complexity}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-sm text-muted-foreground">{fn.file}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>

        {/* Top Connected Nodes Table */}
        <Card>
          <CardHeader>
            <CardTitle>Most Connected Nodes</CardTitle>
            <CardDescription>Nodes with highest degree centrality</CardDescription>
          </CardHeader>
          <CardContent>
            <div className="overflow-x-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Node</TableHead>
                    <TableHead>Type</TableHead>
                    <TableHead>In Degree</TableHead>
                    <TableHead>Out Degree</TableHead>
                    <TableHead>PageRank</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {centrality.slice(0, 10).map((node, i) => (
                    <TableRow key={i}>
                      <TableCell className="font-mono text-sm">{node.name}</TableCell>
                      <TableCell>
                        <Badge variant="outline">{node.type}</Badge>
                      </TableCell>
                      <TableCell>{node.in_degree}</TableCell>
                      <TableCell>{node.out_degree}</TableCell>
                      <TableCell>{node.pagerank.toFixed(4)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
