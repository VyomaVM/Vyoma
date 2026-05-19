import { Outlet, useLocation } from 'react-router-dom';
import { SidebarItem } from '../components/ui';

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

  return (
    <div className="flex h-screen bg-background text-foreground font-sans overflow-hidden">
      <aside className="w-64 bg-sidebar border-r border-sidebar-border flex flex-col shrink-0">
        <div className="p-5 flex items-center gap-3 border-b border-sidebar-border/80">
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
        </div>

        <nav className="flex-1 p-3 space-y-1 overflow-y-auto">
          {tabs.map((tab, index) => {
            const isGroupEnd = index === 3 || index === 7;
            const active = currentPath.startsWith(tab.path) || (tab.path === '/' && currentPath === '/');
            return (
              <div key={tab.id}>
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

      <main className="flex-1 overflow-auto bg-background relative">
        <Outlet />
      </main>
    </div>
  );
}
