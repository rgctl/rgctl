import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';

export function Security() {
  return (
    <div className="p-6 overflow-y-auto h-full">
      <div className="max-w-7xl mx-auto space-y-6">
        <div>
          <h1 className="text-2xl font-bold text-primary mb-2">Security Analysis</h1>
          <p className="text-muted-foreground">
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
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
              <Input placeholder="File path (e.g., src/auth.py)" />
              <Input placeholder="Function name" />
              <Input placeholder="Language (optional)" />
            </div>
            <div className="flex gap-2">
              <Button>Run Taint Analysis</Button>
              <Button variant="secondary">Security Scan</Button>
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
            <div className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4">
              <Input placeholder="File path" />
              <Input type="number" placeholder="Line number" />
              <Input placeholder="Variable name" />
            </div>
            <Button>Run Slice</Button>
          </CardContent>
        </Card>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Severity Levels</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <div className="flex items-center gap-2">
                <Badge variant="destructive">Critical</Badge>
                <span className="text-xs text-muted-foreground">9-10</span>
              </div>
              <div className="flex items-center gap-2">
                <Badge className="bg-yellow-600">High</Badge>
                <span className="text-xs text-muted-foreground">7-8</span>
              </div>
              <div className="flex items-center gap-2">
                <Badge variant="secondary">Medium</Badge>
                <span className="text-xs text-muted-foreground">4-6</span>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Common Vulnerabilities</CardTitle>
            </CardHeader>
            <CardContent className="space-y-1">
              <p className="text-xs text-muted-foreground">• SQL Injection (CWE-89)</p>
              <p className="text-xs text-muted-foreground">• XSS (CWE-79)</p>
              <p className="text-xs text-muted-foreground">• Path Traversal (CWE-22)</p>
              <p className="text-xs text-muted-foreground">• Command Injection (CWE-78)</p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Analysis Status</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground">
                Ready to analyze. Enter file and function details above.
              </p>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
