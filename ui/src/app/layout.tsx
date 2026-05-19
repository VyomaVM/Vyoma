import type { ReactNode } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import { SidebarItem } from '../components/ui';

export const tabs = [
  { id: 'vms', label: 'MicroVMs', icon: 'box', path: '/vms' },
  { id: 'images', label: 'Images', icon: 'hard-drive', path: '/images' },
  { id: 'volumes', label: 'Volumes', icon: 'database', path: '/volumes' },
  { id: 'networks', label: 'Networks', icon: 'globe', path: '/networks' },
  { id: 'timemachine', label: 'TimeMachine', icon: 'history', path: '/timemachine' },
  { id: 'topology', label: 'Topology', icon: 'git-branch', path: '/topology' },
  { id: 'compose', label: 'Compose Editor', icon: 'code', path: '/compose' },
  { id: 'hub', label: 'Hub Browser', icon: 'search', path: '/hub' },
  { id: 'cluster', label: 'Cluster', icon: 'activity', path: '/cluster' },
  { id: 'builds', label: 'Builds', icon: 'activity', path: '/builds' },
  { id: 'events', label: 'Events', icon: 'activity', path: '/events' },
  { id: 'attestation', label: 'Attestation', icon: 'activity', path: '/attestation' },
  { id: 'settings', label: 'Settings', icon: 'settings', path: '/settings' },
];

const iconMap: Record<string, ReactNode> = {
  box: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" /></svg>,
  'hard-drive': <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="22" y1="12" x2="2" y2="12" /><path d="M5.45 5.11L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /><line x1="6" y1="16" x2="6.01" y2="16" /><line x1="10" y1="16" x2="10.01" y2="16" /></svg>,
  database: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><ellipse cx="12" cy="5" rx="9" ry="3" /><path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3" /><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5" /></svg>,
  globe: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="10" /><line x1="2" y1="12" x2="22" y2="12" /><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" /></svg>,
  history: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /><path d="M12 7v5l4 2" /></svg>,
  'git-branch': <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><line x1="6" y1="3" x2="6" y2="15" /><circle cx="18" cy="6" r="3" /><circle cx="6" cy="18" r="3" /><path d="M18 9a9 9 0 0 1-9 9" /></svg>,
  code: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="16 18 22 12 16 6" /><polyline points="8 6 2 12 8 18" /></svg>,
  search: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /></svg>,
  activity: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12" /></svg>,
  settings: <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" /></svg>,
};

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
                  icon={iconMap[tab.icon]}
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
