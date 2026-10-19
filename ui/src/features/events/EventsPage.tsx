import { useState, useRef, useEffect } from 'react';
import { useEventStream } from '../../hooks/useEventStream';
import { Zap, Circle, Cpu, Send, HardDrive, Filter } from 'lucide-react';
import { Card, Input } from '../../components/ui';

export function EventsPage() {
  const { events, isConnected } = useEventStream('/events');
  const [filter, setFilter] = useState('');
  const [activeCategory, setActiveCategory] = useState<'all' | 'vmm' | 'teleport' | 'disk' | 'lifecycle'>('all');
  const logEndRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  const filteredEvents = events.filter(e => {
    // Category filter
    if (activeCategory === 'vmm' && e.type !== 'ch_vmm_event') return false;
    if (activeCategory === 'teleport' && !e.type?.includes('teleport')) return false;
    if (activeCategory === 'disk' && !e.type?.includes('disk')) return false;
    if (activeCategory === 'lifecycle' && !['vm_start', 'vm_stop', 'vm_pause', 'vm_resume'].includes(e.type || '')) return false;

    // Text search filter
    if (!filter) return true;
    const searchLower = filter.toLowerCase();
    return (
      e.type?.toLowerCase().includes(searchLower) ||
      JSON.stringify(e.data).toLowerCase().includes(searchLower)
    );
  });

  useEffect(() => {
    if (autoScroll) {
      logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [filteredEvents, autoScroll]);

  const renderEventDetails = (ev: any) => {
    if (ev.type === 'ch_vmm_event') {
      const data = typeof ev.data === 'string' ? JSON.parse(ev.data) : ev.data;
      const eventName = data?.event || 'vmm_event';
      const source = data?.source || 'vmm';
      const vmId = data?.vm_id ? `vm:${data.vm_id.slice(0, 8)}` : '';

      return (
        <div className="flex flex-wrap items-center gap-2">
          <span className="px-2 py-0.5 rounded text-xs font-semibold bg-purple-500/20 text-purple-400 border border-purple-500/30 flex items-center gap-1">
            <Cpu size={12} /> CH v54 {source}
          </span>
          <span className={`px-2 py-0.5 rounded text-xs font-semibold uppercase ${
            eventName === 'started' || eventName === 'resumed' ? 'bg-green-500/20 text-green-400 border border-green-500/30' :
            eventName === 'paused' ? 'bg-yellow-500/20 text-yellow-400 border border-yellow-500/30' :
            eventName === 'migration_cancelled' ? 'bg-amber-500/20 text-amber-400 border border-amber-500/30' :
            'bg-muted text-muted-foreground border border-border'
          }`}>
            {eventName}
          </span>
          {vmId && (
            <span className="font-mono text-xs text-muted-foreground">
              [{vmId}]
            </span>
          )}
          {data?.properties && (
            <span className="text-xs text-muted-foreground font-mono">
              {JSON.stringify(data.properties)}
            </span>
          )}
        </div>
      );
    }

    if (ev.type === 'teleport_cancelled' || ev.type?.includes('teleport')) {
      const data = typeof ev.data === 'string' ? (tryParse(ev.data) || ev.data) : ev.data;
      return (
        <div className="flex items-center gap-2">
          <span className="px-2 py-0.5 rounded text-xs font-semibold bg-red-500/20 text-red-400 border border-red-500/30 flex items-center gap-1">
            <Send size={12} /> {ev.type}
          </span>
          <span className="text-foreground">
            {typeof data === 'object' ? JSON.stringify(data) : data}
          </span>
        </div>
      );
    }

    if (ev.type === 'disk_resized') {
      const data = typeof ev.data === 'string' ? (tryParse(ev.data) || ev.data) : ev.data;
      return (
        <div className="flex items-center gap-2">
          <span className="px-2 py-0.5 rounded text-xs font-semibold bg-cyan-500/20 text-cyan-400 border border-cyan-500/30 flex items-center gap-1">
            <HardDrive size={12} /> disk_resized
          </span>
          <span className="text-foreground font-mono text-xs">
            {typeof data === 'object' ? JSON.stringify(data) : data}
          </span>
        </div>
      );
    }

    return (
      <span className="text-foreground break-all">
        {typeof ev.data === 'string' ? ev.data : JSON.stringify(ev.data)}
      </span>
    );
  };

  const tryParse = (str: string) => {
    try {
      return JSON.parse(str);
    } catch {
      return null;
    }
  };

  return (
    <div className="p-8 max-w-6xl mx-auto h-[calc(100vh-4rem)] flex flex-col">
      <header className="mb-4 flex flex-wrap items-center justify-between gap-4 shrink-0">
        <div>
          <h2 className="text-2xl font-bold text-foreground mb-1 flex items-center gap-2">
            <Zap className="text-primary" /> System & Hypervisor Events
          </h2>
          <p className="text-sm text-muted-foreground">Real-time event feed from Cloud Hypervisor v54.0 and Vyoma daemon.</p>
        </div>
        <div className="flex items-center gap-4">
          <Input 
            value={filter} 
            onChange={(e) => setFilter(e.target.value)} 
            placeholder="Filter events..." 
            className="w-56"
          />
          <div className="flex items-center gap-2 text-sm">
            <label className="flex items-center gap-2 cursor-pointer text-muted-foreground">
              <input 
                type="checkbox" 
                checked={autoScroll} 
                onChange={(e) => setAutoScroll(e.target.checked)} 
                className="rounded border-border bg-sidebar"
              />
              Auto-scroll
            </label>
            <div className={`flex items-center gap-1.5 ml-4 ${isConnected ? 'text-green-500' : 'text-red-500'}`}>
              <Circle size={10} fill="currentColor" /> {isConnected ? 'Connected' : 'Disconnected'}
            </div>
          </div>
        </div>
      </header>

      {/* Filter Tabs */}
      <div className="flex items-center gap-2 mb-3 shrink-0">
        <Filter size={14} className="text-muted-foreground mr-1" />
        {(['all', 'vmm', 'lifecycle', 'teleport', 'disk'] as const).map((cat) => (
          <button
            key={cat}
            onClick={() => setActiveCategory(cat)}
            className={`px-3 py-1 rounded-md text-xs font-semibold capitalize transition ${
              activeCategory === cat
                ? 'bg-primary text-primary-foreground'
                : 'bg-card border border-border text-muted-foreground hover:text-foreground hover:bg-muted'
            }`}
          >
            {cat === 'all' ? 'All Events' : cat === 'vmm' ? 'CH v54 VMM' : cat}
          </button>
        ))}
      </div>

      <Card className="flex-1 overflow-auto bg-card border-border font-mono text-sm relative">
        <div className="p-4 space-y-2">
          {filteredEvents.length === 0 ? (
            <div className="text-center py-12 text-muted-foreground">
              No events recorded matching the current filter.
            </div>
          ) : (
            filteredEvents.map(ev => (
              <div key={ev.id} className="flex gap-4 p-2 hover:bg-muted/50 rounded transition items-center">
                <div className="text-muted-foreground shrink-0 w-24 text-xs">
                  {new Date(ev.timestamp).toLocaleTimeString()}
                </div>
                <div className="text-primary font-bold shrink-0 w-36 truncate text-xs" title={ev.type}>
                  {ev.type || 'unknown'}
                </div>
                <div className="flex-1 min-w-0">
                  {renderEventDetails(ev)}
                </div>
              </div>
            ))
          )}
          <div ref={logEndRef} />
        </div>
      </Card>
    </div>
  );
}
