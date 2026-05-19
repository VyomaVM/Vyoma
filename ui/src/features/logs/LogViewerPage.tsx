import { useParams } from 'react-router-dom';
import { useState, useRef, useEffect } from 'react';
import { Terminal, Circle } from 'lucide-react';
import { useEventStream } from '../../hooks/useEventStream';

export function LogViewerPage() {
  const { vmId } = useParams();
  const { events, isConnected } = useEventStream(`/logs/${vmId}`);
  const logEndRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    if (autoScroll) {
      logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [events, autoScroll]);

  return (
    <div className="p-8 max-w-6xl mx-auto h-[calc(100vh-4rem)] flex flex-col">
      <header className="mb-4 flex items-center justify-between shrink-0">
        <div>
          <h2 className="text-2xl font-bold text-foreground mb-1 flex items-center gap-2">
            <Terminal className="text-primary" /> VM Logs
          </h2>
          <p className="text-sm text-muted-foreground font-mono">ID: {vmId}</p>
        </div>
        <div className="flex items-center gap-4">
          <label className="flex items-center gap-2 cursor-pointer text-sm text-muted-foreground">
            <input 
              type="checkbox" 
              checked={autoScroll} 
              onChange={(e) => setAutoScroll(e.target.checked)} 
              className="rounded"
            />
            Follow
          </label>
          <div className={`flex items-center gap-1.5 text-sm ${isConnected ? 'text-green-500' : 'text-red-500'}`}>
            <Circle size={10} fill="currentColor" /> {isConnected ? 'Connected' : 'Disconnected'}
          </div>
        </div>
      </header>

      <div className="flex-1 bg-[#0d0d18] rounded-xl border border-border p-4 overflow-auto font-mono text-xs text-slate-300 shadow-inner">
        {events.length === 0 ? (
          <div className="text-slate-500 italic">Waiting for logs...</div>
        ) : (
          <div className="space-y-1">
            {events.map((ev, i) => (
              <div key={i} className="break-all whitespace-pre-wrap hover:bg-white/5 px-1 rounded transition-colors">
                <span className="text-slate-500 mr-4 select-none border-r border-slate-700 pr-4">
                  {new Date(ev.timestamp).toISOString().split('T')[1].slice(0, -1)}
                </span>
                <span className={ev.type === 'error' ? 'text-red-400' : ev.type === 'warn' ? 'text-yellow-400' : 'text-slate-300'}>
                  {typeof ev.data === 'string' ? ev.data : JSON.stringify(ev.data)}
                </span>
              </div>
            ))}
            <div ref={logEndRef} />
          </div>
        )}
      </div>
    </div>
  );
}
