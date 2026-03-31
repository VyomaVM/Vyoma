import type { ReactNode } from 'react';
import { SidebarItem } from './ui';

interface LayoutProps {
  children: ReactNode;
  activeTab: string;
  onTabChange: (tab: string) => void;
}

const tabs = [
  { id: 'vms', label: 'MicroVMs', icon: 'box' },
  { id: 'images', label: 'Images', icon: 'hard-drive' },
  { id: 'volumes', label: 'Volumes', icon: 'database' },
  { id: 'networks', label: 'Networks', icon: 'globe' },
  { id: 'timemachine', label: 'TimeMachine', icon: 'history' },
  { id: 'topology', label: 'Topology', icon: 'git-branch' },
  { id: 'compose', label: 'Compose Editor', icon: 'code' },
  { id: 'hub', label: 'Hub Browser', icon: 'search' },
  { id: 'stats', label: 'Cluster Stats', icon: 'activity' },
  { id: 'settings', label: 'Settings', icon: 'settings' },
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

export function Layout({ children, activeTab, onTabChange }: LayoutProps) {
  return (
    <div className="flex h-screen bg-slate-950 text-slate-200 font-sans overflow-hidden">
      <aside className="w-64 bg-slate-900 border-r border-slate-800 flex flex-col shrink-0">
        <div className="p-5 flex items-center gap-3 border-b border-slate-800/80">
          <div className="w-8 h-8 rounded-lg flex items-center justify-center">
            <svg width="32" height="32" viewBox="0 0 64 64" className="w-full h-full">
              <defs>
                <linearGradient id="flame" x1="0%" y1="100%" x2="0%" y2="0%">
                  <stop offset="0%" stopColor="#FF4500"/>
                  <stop offset="50%" stopColor="#FF6B2B"/>
                  <stop offset="100%" stopColor="#FFA500"/>
                </linearGradient>
              </defs>
              <path d="M32 4C32 4 18 20 18 36c0 7.7 6.3 14 14 14s14-6.3 14-14C46 20 32 4 32 4zm0 40c-4.4 0-8-3.6-8-8 0-6 8-16 8-16s8 10 8 16c0 4.4-3.6 8-8 8z" fill="url(#flame)"/>
            </svg>
          </div>
          <h1 className="font-bold text-lg tracking-tight text-white">Ignite</h1>
        </div>

        <nav className="flex-1 p-3 space-y-1 overflow-y-auto">
          {tabs.slice(0, 4).map((tab) => (
            <SidebarItem
              key={tab.id}
              icon={iconMap[tab.icon]}
              label={tab.label}
              active={activeTab === tab.id}
              onClick={() => onTabChange(tab.id)}
            />
          ))}

          <div className="my-4 border-t border-slate-800 mx-2" />

          {tabs.slice(4, 8).map((tab) => (
            <SidebarItem
              key={tab.id}
              icon={iconMap[tab.icon]}
              label={tab.label}
              active={activeTab === tab.id}
              onClick={() => onTabChange(tab.id)}
            />
          ))}

          <div className="my-4 border-t border-slate-800 mx-2" />

          {tabs.slice(8).map((tab) => (
            <SidebarItem
              key={tab.id}
              icon={iconMap[tab.icon]}
              label={tab.label}
              active={activeTab === tab.id}
              onClick={() => onTabChange(tab.id)}
            />
          ))}
        </nav>

        <div className="p-4 border-t border-slate-800 bg-slate-925">
          <div className="flex items-center gap-3 rounded-lg bg-slate-800/50 p-3 border border-slate-800">
            <div className="relative">
              <div className="w-3 h-3 rounded-full bg-green-500 animate-pulse border-2 border-slate-900" />
            </div>
            <div className="text-xs">
              <div className="text-slate-300 font-medium">Daemon Active</div>
              <div className="text-slate-500">v1.9.0</div>
            </div>
          </div>
        </div>
      </aside>

      <main className="flex-1 overflow-auto bg-slate-950 relative">
        {children}
      </main>
    </div>
  );
}
