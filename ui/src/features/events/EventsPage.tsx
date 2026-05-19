import { useState, useRef, useEffect } from 'react';
import { useEventStream } from '../../hooks/useEventStream';
import { Zap, Circle } from 'lucide-react';
import { Card, Input } from '../../components/ui';

export function EventsPage() {
  const { events, isConnected } = useEventStream('/events');
  const [filter, setFilter] = useState('');
  const logEndRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  const filteredEvents = events.filter(e => 
    !filter || e.type?.includes(filter) || JSON.stringify(e.data).includes(filter)
  );

  useEffect(() => {
    if (autoScroll) {
      logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [filteredEvents, autoScroll]);

  return (
    <div className="p-8 max-w-6xl mx-auto h-[calc(100vh-4rem)] flex flex-col">
      <header className="mb-4 flex items-center justify-between shrink-0">
        <div>
          <h2 className="text-2xl font-bold text-foreground mb-1 flex items-center gap-2">
            <Zap className="text-primary" /> System Events
          </h2>
          <p className="text-sm text-muted-foreground">Live streaming event log from the daemon.</p>
        </div>
        <div className="flex items-center gap-4">
          <Input 
            value={filter} 
            onChange={(e) => setFilter(e.target.value)} 
            placeholder="Filter events..." 
            className="w-64"
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

      <Card className="flex-1 overflow-auto bg-card border-border font-mono text-sm relative">
        <div className="p-4 space-y-2">
          {filteredEvents.map(ev => (
            <div key={ev.id} className="flex gap-4 p-2 hover:bg-muted/50 rounded transition">
              <div className="text-muted-foreground shrink-0 w-24">
                {new Date(ev.timestamp).toLocaleTimeString()}
              </div>
              <div className="text-primary font-bold shrink-0 w-32 truncate" title={ev.type}>
                {ev.type || 'unknown'}
              </div>
              <div className="text-foreground break-all">
                {typeof ev.data === 'string' ? ev.data : JSON.stringify(ev.data)}
              </div>
            </div>
          ))}
          <div ref={logEndRef} />
        </div>
      </Card>
    </div>
  );
}
