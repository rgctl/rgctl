import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';

export function Security() {
  return (
    <div className="p-8 overflow-y-auto h-full bg-background">
      <div className="space-y-8">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Security Analysis</h1>
          <p className="text-muted-foreground mt-2">
            Taint analysis, vulnerability scanning, and program slicing
          </p>
        </div>

        <Card>
          <CardHeader>
            <CardTitle>Taint Analysis & Vulnerability Scan</CardTitle>
            <CardDescription>
              Track untrusted data flows from sources to security-sensitive sinks
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col gap-6">
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <Input placeholder="File path (e.g., src/auth.py)" />
                <Input placeholder="Function name" />
                <Input placeholder="Language (optional)" />
              </div>
              <div className="flex gap-2">
                <Button>Run Taint Analysis</Button>
                <Button variant="secondary">Security Scan</Button>
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Backward Program Slice</CardTitle>
            <CardDescription>
              Find all code that affects a variable at a specific line
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex flex-col gap-6">
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <Input placeholder="File path" />
                <Input type="number" placeholder="Line number" />
                <Input placeholder="Variable name" />
              </div>
              <Button>Run Slice</Button>
            </div>
          </CardContent>
        </Card>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-sm font-semibold">Severity Levels</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="flex flex-col gap-2">
                <div className="flex items-center gap-2">
                  <Badge variant="destructive">Critical</Badge>
                  <span className="text-xs text-muted-foreground">9-10</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="default">High</Badge>
                  <span className="text-xs text-muted-foreground">7-8</span>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="secondary">Medium</Badge>
                  <span className="text-xs text-muted-foreground">4-6</span>
                </div>
              </div>
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-sm font-semibold">Common Vulnerabilities</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="flex flex-col gap-1">
                <div className="text-xs text-muted-foreground">• SQL Injection (CWE-89)</div>
                <div className="text-xs text-muted-foreground">• XSS (CWE-79)</div>
                <div className="text-xs text-muted-foreground">• Path Traversal (CWE-22)</div>
                <div className="text-xs text-muted-foreground">• Command Injection (CWE-78)</div>
              </div>
            </CardContent>
          </Card>

          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-sm font-semibold">Analysis Status</CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-sm text-muted-foreground">
                Ready to analyze. Enter file and function details above.
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
