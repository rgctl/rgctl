import { Link, useLocation } from 'react-router-dom';

export function Layout({ children }: { children: React.ReactNode }) {
  const location = useLocation();

  const isActive = (path: string) => location.pathname === path;

  return (
    <div className="flex flex-col h-screen bg-[#0f1117] text-[#e6edf3]">
      <header className="flex items-center gap-3 px-5 py-3 bg-[#161b22] border-b border-[#30363d] flex-wrap">
        <h1 className="text-lg font-medium text-[#58a6ff] mr-auto">rBuilder</h1>

        <nav className="flex items-center gap-4 text-sm">
          <Link
            to="/"
            className={`${
              isActive('/') ? 'text-[#58a6ff]' : 'text-[#e6edf3] hover:text-[#58a6ff]'
            } transition-colors`}
          >
            Graph Browser
          </Link>
          <Link
            to="/dashboard"
            className={`${
              isActive('/dashboard') ? 'text-[#58a6ff]' : 'text-[#e6edf3] hover:text-[#58a6ff]'
            } transition-colors`}
          >
            Dashboard
          </Link>
          <Link
            to="/security"
            className={`${
              isActive('/security') ? 'text-[#58a6ff]' : 'text-[#e6edf3] hover:text-[#58a6ff]'
            } transition-colors`}
          >
            Security
          </Link>
        </nav>
      </header>

      <main className="flex-1 overflow-hidden">{children}</main>
    </div>
  );
}
