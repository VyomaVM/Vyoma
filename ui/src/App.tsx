import { useEffect, useState } from 'react'
import { Activity, Server, Terminal, Square, RefreshCw } from 'lucide-react'
import clsx from 'clsx'

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

interface VM {
  id: string;
  labels: Record<string, string>;
}

function App() {
  const [vms, setVms] = useState<VM[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchVms = async () => {
    try {
      const res = await fetch(`${API_BASE}/ps`);
      const data = await res.json();
      setVms(data.vms || []);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchVms();
    // Setup SSE
    const evt = new EventSource(`${API_BASE}/events`);
    evt.onmessage = (e) => {
      console.log("SSE Update", e.data);
      fetchVms(); // Simple refresh on any event
    };
    evt.onerror = () => {
      // Retry logic handled by browser usually, just don't crash
    };
    return () => evt.close();
  }, []);

  return (
    <div className="min-h-screen bg-slate-950 text-slate-200 font-sans">
      <nav className="border-b border-slate-800 bg-slate-900/50 backdrop-blur p-4 flex items-center justify-between sticky top-0 z-50">
        <div className="flex items-center gap-2">
          <div className="w-8 h-8 bg-orange-600 rounded-lg flex items-center justify-center shadow-lg shadow-orange-900/20">
            <span className="font-bold text-white">I</span>
          </div>
          <h1 className="text-xl font-bold bg-gradient-to-r from-orange-400 to-red-500 text-transparent bg-clip-text">Ignite Ecosystem</h1>
        </div>
        <div className="flex gap-4 text-sm font-medium text-slate-400">
          <div className="flex items-center gap-2 px-3 py-1 bg-slate-900 rounded-full border border-slate-800">
            <div className="w-2 h-2 rounded-full bg-green-500 animate-pulse"></div>
            <span>Cluster: Local</span>
          </div>
          <span className="py-1 px-2">v0.8.0</span>
        </div>
      </nav>

      <main className="max-w-7xl mx-auto p-8">
        <div className="flex items-center justify-between mb-8">
          <h2 className="text-2xl font-bold text-white flex items-center gap-3">
            <Server className="w-6 h-6 text-blue-400" />
            Running MicroVMs
          </h2>
          <button onClick={fetchVms} className="p-2 bg-slate-800 rounded-lg hover:bg-slate-700 transition hover:rotate-180 duration-500">
            <RefreshCw className="w-5 h-5" />
          </button>
        </div>

        <div className="grid gap-4">
          {loading ? (
            <div className="text-center py-20 text-slate-500 animate-pulse">Connecting to Daemon...</div>
          ) : vms.length === 0 ? (
            <div className="text-center py-20 bg-slate-900/50 rounded-xl border border-slate-800 border-dashed">
              <p className="text-lg text-slate-300">No VMs running.</p>
              <p className="text-sm text-slate-500 mt-2">Run <code className="bg-slate-950 px-2 py-1 rounded text-orange-400">ign run ubuntu</code> to start one.</p>
            </div>
          ) : (
            vms.map(vm => (
              <div key={vm.id} className="bg-slate-900 border border-slate-800 rounded-xl p-6 flex items-center justify-between group hover:border-orange-500/30 transition shadow-lg shadow-black/20">
                <div className="flex items-center gap-4">
                  <div className="w-12 h-12 rounded-xl bg-gradient-to-br from-slate-800 to-slate-900 border border-slate-700 flex items-center justify-center text-green-400 shadow-inner">
                    <Activity className="w-6 h-6" />
                  </div>
                  <div>
                    <h3 className="text-lg font-bold text-white tracking-tight">
                      {vm.labels['ignite.service'] || "MicroVM"}
                    </h3>
                    <div className="flex gap-2 text-xs text-slate-400 mt-1 font-mono">
                      <span className="bg-slate-950 px-2 py-0.5 rounded border border-slate-800">{vm.id.substring(0, 12)}</span>
                      <span className="text-green-400 flex items-center gap-1">
                        <span className="w-1.5 h-1.5 rounded-full bg-green-500"></span>
                        Running
                      </span>
                    </div>
                  </div>
                </div>

                <div className="flex gap-3 opacity-60 group-hover:opacity-100 transition duration-300">
                  <Button icon={<Terminal className="w-4 h-4" />} label="Console" />
                  <Button icon={<Square className="w-4 h-4 fill-current" />} label="Stop" variant="danger"
                    onClick={async () => {
                      await fetch(`${API_BASE}/stop/${vm.id}`, { method: 'POST' });
                    }}
                  />
                </div>
              </div>
            ))
          )}
        </div>
      </main>
    </div>
  )
}

function Button({ icon, label, variant = 'primary', onClick }: any) {
  const base = "flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition active:scale-95 border";
  const styles = variant === 'danger'
    ? "bg-red-500/5 border-red-500/20 text-red-400 hover:bg-red-500/10 hover:border-red-500/30"
    : "bg-slate-800 border-slate-700 text-slate-200 hover:bg-slate-700 hover:text-white hover:border-slate-600";

  return (
    <button className={clsx(base, styles)} onClick={onClick}>
      {icon}
      {label}
    </button>
  )
}

export default App
