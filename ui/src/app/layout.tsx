import { useRef } from 'react';
import { Outlet, useLocation, Link } from 'react-router-dom';
import { Menu, X } from 'lucide-react';
import { SidebarItem, ScrollArea } from '../components/ui';
import { useUIStore } from '../stores/ui.store';
import { ErrorBoundary } from '../components/ErrorBoundary';
import { useFocusOnNavigate } from '../hooks/useFocusOnNavigate';

import { 
  Box, HardDrive, Database, Globe, History, GitBranch, 
  Code, Search, Server, Wrench, Zap, ShieldCheck, Settings 
} from 'lucide-react';

export const tabs = [
  { id: 'vms', label: 'MicroVMs', icon: <Box size={18} />, path: '/vms' },
  { id: 'images', label: 'Images', icon: <HardDrive size={18} />, path: '/images' },
  { id: 'volumes', label: 'Volumes', icon: <Database size={18} />, path: '/volumes' },
  { id: 'networks', label: 'Networks', icon: <Globe size={18} />, path: '/networks' },
  { id: 'timemachine', label: 'TimeMachine', icon: <History size={18} />, path: '/timemachine' },
  { id: 'topology', label: 'Topology', icon: <GitBranch size={18} />, path: '/topology' },
  { id: 'compose', label: 'Compose Editor', icon: <Code size={18} />, path: '/compose' },
  { id: 'hub', label: 'Hub Browser', icon: <Search size={18} />, path: '/hub' },
  { id: 'cluster', label: 'Cluster', icon: <Server size={18} />, path: '/cluster' },
  { id: 'builds', label: 'Builds', icon: <Wrench size={18} />, path: '/builds' },
  { id: 'events', label: 'Events', icon: <Zap size={18} />, path: '/events' },
  { id: 'attestation', label: 'Attestation', icon: <ShieldCheck size={18} />, path: '/attestation' },
  { id: 'settings', label: 'Settings', icon: <Settings size={18} />, path: '/settings' },
];

export function AppLayout() {
  const location = useLocation();
  const currentPath = location.pathname;
  const mainRef = useRef<HTMLElement>(null);
  const { sidebarOpen, setSidebarOpen } = useUIStore();

  useFocusOnNavigate(mainRef);

  const closeSidebarOnMobile = () => {
    if (window.innerWidth < 768) {
      setSidebarOpen(false);
    }
  };

  return (
    <div className="flex h-screen bg-background text-foreground font-sans overflow-hidden">
      
      {/* Mobile Sidebar Overlay */}
      {sidebarOpen && (
        <div 
          className="fixed inset-0 bg-black/50 z-40 md:hidden transition-opacity"
          onClick={() => setSidebarOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Sidebar */}
      <aside 
        className={`fixed md:relative z-50 w-64 h-full bg-sidebar border-r border-sidebar-border flex flex-col shrink-0 transition-transform duration-300 ease-in-out ${
          sidebarOpen ? 'translate-x-0' : '-translate-x-full md:translate-x-0'
        }`}
      >
        <div className="p-5 flex items-center justify-between gap-3 border-b border-sidebar-border/80">
          <Link to="/" className="flex items-center gap-3 hover:opacity-80 transition-opacity focus:outline-none focus:ring-2 focus:ring-primary rounded-lg" onClick={closeSidebarOnMobile}>
            <div className="w-8 h-8 rounded-lg flex items-center justify-center">
              <svg className="nav__icon" width="28" height="28" viewBox="0 0 64 64" fill="none">
                <defs>
                  <radialGradient id="navSphere" cx="38%" cy="32%" r="65%">
                    <stop offset="0%" stopColor="#fdd835"/>
                    <stop offset="60%" stopColor="#d4a017"/>
                    <stop offset="100%" stopColor="#7a5500"/>
                  </radialGradient>
                  <linearGradient id="navOrbit" x1="0%" y1="0%" x2="100%" y2="100%">
                    <stop offset="0%" stopColor="#67e8f9"/>
                    <stop offset="100%" stopColor="#3b82f6"/>
                  </linearGradient>
                </defs>
                <circle cx="32" cy="32" r="24" fill="#0d0d18" stroke="#1e1e38" strokeWidth="1"/>
                <circle cx="32" cy="32" r="20" fill="url(#navSphere)" opacity="0.9"/>
                <g stroke="#04040a" strokeWidth="1.4" fill="none" opacity="0.6">
                  <line x1="32" y1="12" x2="32" y2="52"/>
                  <line x1="12" y1="32" x2="52" y2="32"/>
                  <line x1="18" y1="18" x2="46" y2="46"/>
                  <line x1="46" y1="18" x2="18" y2="46"/>
                  <ellipse cx="32" cy="32" rx="20" ry="9"/>
                  <ellipse cx="32" cy="32" rx="9" ry="20"/>
                </g>
                <circle cx="32" cy="32" r="3" fill="#fdd835"/>
                <ellipse cx="32" cy="32" rx="30" ry="9" fill="none"
                  stroke="url(#navOrbit)" strokeWidth="2.5"
                  transform="rotate(-25 32 32)" opacity="0.9"/>
              </svg>
            </div>
            <h1 className="font-bold text-lg tracking-tight text-foreground">Vyoma</h1>
          </Link>
          <button 
            className="md:hidden text-muted-foreground hover:text-foreground p-1 rounded-md" 
            onClick={() => setSidebarOpen(false)} 
            aria-label="Close sidebar"
          >
            <X size={20} />
          </button>
        </div>

        <ScrollArea className="flex-1">
          <nav className="p-3 space-y-1">
            {tabs.map((tab, index) => {
              const isGroupEnd = index === 3 || index === 7;
              const active = currentPath.startsWith(tab.path) || (tab.path === '/' && currentPath === '/');
              return (
                <div key={tab.id} onClick={closeSidebarOnMobile}>
                  <SidebarItem
                    icon={tab.icon}
                    label={tab.label}
                    active={active}
                    to={tab.path}
                  />
                  {isGroupEnd && <div className="my-4 border-t border-sidebar-border mx-2" />}
                </div>
              );
            })}
          </nav>
        </ScrollArea>

        <div className="p-4 border-t border-sidebar-border bg-sidebar">
          <div className="flex items-center gap-3 rounded-lg bg-card/50 p-3 border border-border">
            <div className="relative">
              <div className="w-3 h-3 rounded-full bg-green-500 animate-pulse border-2 border-background" />
            </div>
            <div className="text-xs">
              <div className="text-foreground font-medium">Daemon Active</div>
              <div className="text-muted-foreground">v2.1.2</div>
            </div>
          </div>
        </div>
      </aside>

      {/* Main Content Area */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden relative">
        {/* Mobile Header */}
        <header className="md:hidden h-14 border-b border-border bg-card flex items-center px-4 shrink-0">
          <button 
            onClick={() => setSidebarOpen(true)}
            className="text-foreground p-2 -ml-2 rounded-md hover:bg-muted"
            aria-label="Open sidebar"
          >
            <Menu size={20} />
          </button>
          <span className="font-bold ml-2">Vyoma</span>
        </header>

        <main 
          ref={mainRef}
          tabIndex={-1}
          aria-label="Main content"
          className="flex-1 overflow-auto bg-background relative focus:outline-none"
        >
          <ErrorBoundary>
            <Outlet />
          </ErrorBoundary>
        </main>
      </div>
    </div>
  );
}
