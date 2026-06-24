import { Link, useLocation } from 'react-router-dom';
import { Moon, Sun } from 'lucide-react';
import { useTheme } from './theme-provider';
import { cn } from '@/lib/utils';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';

export function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const { setTheme } = useTheme();

  const isActive = (path: string) => location.pathname === path;

  const navLinks = [
    { path: '/', label: 'Graph Browser' },
    { path: '/dashboard', label: 'Dashboard' },
    { path: '/security', label: 'Security' },
  ];

  return (
    <div className="flex flex-col h-screen bg-background text-foreground">
      <header className="flex items-center gap-3 px-5 py-3 bg-card border-b flex-wrap">
        <div className="mr-auto flex items-center gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 60" className="h-10">
            <defs>
              <linearGradient id="redGrad" x1="0%" y1="0%" x2="100%" y2="0%">
                <stop offset="0%" style={{stopColor: 'oklch(0.577 0.245 27.325)', stopOpacity: 1}} />
                <stop offset="100%" style={{stopColor: 'oklch(0.637 0.237 25.331)', stopOpacity: 1}} />
              </linearGradient>
            </defs>
            <g transform="translate(10, 20)">
              <circle cx="10" cy="10" r="4" fill="url(#redGrad)"/>
              <circle cx="25" cy="5" r="4" fill="url(#redGrad)"/>
              <circle cx="25" cy="20" r="4" fill="url(#redGrad)"/>
              <line x1="10" y1="10" x2="25" y2="5" stroke="url(#redGrad)" strokeWidth="1.5" opacity="0.6"/>
              <line x1="10" y1="10" x2="25" y2="20" stroke="url(#redGrad)" strokeWidth="1.5" opacity="0.6"/>
            </g>
            <text x="50" y="38" fontFamily="Inter Variable, system-ui, -apple-system, sans-serif" fontWeight="700" letterSpacing="-0.5">
              <tspan fill="url(#redGrad)" fontSize="36">R</tspan>
              <tspan className="fill-foreground" fontSize="22">builder</tspan>
            </text>
          </svg>
        </div>

        <nav className="flex items-center gap-1 text-sm">
          {navLinks.map((link) => (
            <Link
              key={link.path}
              to={link.path}
              className={cn(
                'px-3 py-1.5 rounded-md transition-colors',
                isActive(link.path)
                  ? 'bg-secondary text-secondary-foreground'
                  : 'text-muted-foreground hover:text-foreground hover:bg-secondary/50'
              )}
            >
              {link.label}
            </Link>
          ))}
        </nav>

        <DropdownMenu>
          <DropdownMenuTrigger className="inline-flex items-center justify-center rounded-md border border-input bg-background px-3 py-2 text-sm hover:bg-accent hover:text-accent-foreground">
            <Sun className="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
            <Moon className="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
            <span className="sr-only">Toggle theme</span>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={() => setTheme('light')}>
              Light
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setTheme('dark')}>
              Dark
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setTheme('system')}>
              System
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </header>

      <main className="flex-1 overflow-hidden">{children}</main>
    </div>
  );
}
