import { useEffect, useState } from 'react'
import { Activity, Box, Database, HardDrive, Terminal, Square, Settings, RefreshCw, Globe, Server } from 'lucide-react'
import clsx from 'clsx'

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

function App() {
  const [activeTab, setActiveTab] = useState('vms');

  return (
    <div className="flex h-screen bg-slate-950 text-slate-200 font-sans overflow-hidden">
      {/* Sidebar */}
      <aside className="w-64 bg-slate-900 border-r border-slate-800 flex flex-col shrink-0">
        <div className="p-5 flex items-center gap-3 border-b border-slate-800/80">
          <div className="w-8 h-8 bg-gradient-to-br from-orange-500 to-red-600 rounded-lg flex items-center justify-center shadow-lg shadow-orange-900/20">
            <span className="font-bold text-white">I</span>
          </div>
          <h1 className="font-bold text-lg tracking-tight text-white">Ignite</h1>
        </div>

        <nav className="flex-1 p-3 space-y-1 overflow-y-auto">
          <SidebarItem icon={<Box size={18} />} label="MicroVMs" active={activeTab === 'vms'} onClick={() => setActiveTab('vms')} />
          <SidebarItem icon={<HardDrive size={18} />} label="Images" active={activeTab === 'images'} onClick={() => setActiveTab('images')} />
          <SidebarItem icon={<Database size={18} />} label="Volumes" active={activeTab === 'volumes'} onClick={() => setActiveTab('volumes')} />
          <SidebarItem icon={<Globe size={18} />} label="Networks" active={activeTab === 'networks'} onClick={() => setActiveTab('networks')} />

          <div className="my-4 border-t border-slate-800 mx-2"></div>

          <SidebarItem icon={<Activity size={18} />} label="Cluster Stats" active={activeTab === 'stats'} onClick={() => setActiveTab('stats')} />
          <SidebarItem icon={<Settings size={18} />} label="Settings" active={activeTab === 'settings'} onClick={() => setActiveTab('settings')} />
        </nav>

        <div className="p-4 border-t border-slate-800 bg-slate-925">
          <div className="flex items-center gap-3 rounded-lg bg-slate-800/50 p-3 border border-slate-800">
            <div className="relative">
              <div className="w-3 h-3 rounded-full bg-green-500 animate-pulse border-2 border-slate-900"></div>
            </div>
            <div className="text-xs">
              <div className="text-slate-300 font-medium">Daemon Active</div>
              <div className="text-slate-500">v0.8.0</div>
            </div>
          </div>
        </div>
      </aside>

      {/* Main Content */}
      <main className="flex-1 overflow-auto bg-slate-950 relative">
        {activeTab === 'vms' && <MicroVMsView />}
        {activeTab === 'images' && <ImagesView />}
        {activeTab === 'volumes' && <VolumesView />}
        {activeTab === 'networks' && <NetworksView />}
        {activeTab === 'stats' && <StatsView />}
        {activeTab === 'settings' && <PlaceholderView title="Settings" icon={<Settings size={48} />} />}
      </main>
    </div>
  )
}

function SidebarItem({ icon, label, active, onClick }: { icon: any, label: string, active: boolean, onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      className={clsx(
        "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200",
        active
          ? "bg-orange-500/10 text-orange-400 shadow-sm shadow-orange-900/10 border border-orange-500/10"
          : "text-slate-400 hover:bg-slate-800 hover:text-slate-200"
      )}
    >
      {icon}
      {label}
    </button>
  )
}

function PlaceholderView({ title, icon }: { title: string, icon: any }) {
  return (
    <div className="h-full flex flex-col items-center justify-center text-slate-600 gap-4">
      <div className="p-6 bg-slate-900 rounded-full border border-slate-800">
        {icon}
      </div>
      <h2 className="text-xl font-semibold text-slate-400">{title}</h2>
      <p className="text-sm">This feature is coming soon.</p>
    </div>
  )
}

// --- Views ---

interface VM { id: string; labels: Record<string, string>; status?: string; ip_address?: string; }
interface Volume { name: string; path: string; }
interface Network { name: string; subnet: string; }

function MicroVMsView() {
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
    const evt = new EventSource(`${API_BASE}/events`);
    evt.onmessage = () => fetchVms();
    return () => evt.close();
  }, []);

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="flex items-center justify-between mb-8">
        <div>
          <h2 className="text-2xl font-bold text-white mb-1">MicroVMs</h2>
          <p className="text-sm text-slate-400">Manage your running micro-virtual machines.</p>
        </div>
        <div className="flex gap-3">
          <button onClick={fetchVms} className="p-2 bg-slate-800 border border-slate-700 rounded hover:bg-slate-700 transition text-slate-400 hover:text-white">
            <RefreshCw size={18} />
          </button>
        </div>
      </header>

      <div className="bg-slate-900 rounded-xl border border-slate-800 overflow-hidden shadow-xl shadow-black/20">
        <div className="grid grid-cols-12 gap-4 p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase tracking-wider">
          <div className="col-span-1 flex justify-center">Status</div>
          <div className="col-span-4">Name / ID</div>
          <div className="col-span-3">Image</div>
          <div className="col-span-2">IP Address</div>
          <div className="col-span-2 text-right">Actions</div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {loading ? <div className="p-12 text-center text-slate-500 animate-pulse">Loading...</div> : vms.map(vm => (
            <div key={vm.id} className="grid grid-cols-12 gap-4 p-4 items-center group hover:bg-slate-800/30 transition">
              <div className="col-span-1 flex justify-center"><div className="w-3 h-3 rounded-sm bg-green-500 shadow-[0_0_8px_rgba(34,197,94,0.4)]"></div></div>
              <div className="col-span-4">
                <div className="font-semibold text-white text-sm">{vm.labels['ignite.service'] || "MicroVM"}</div>
                <div className="text-xs text-slate-500 font-mono mt-0.5 truncate" title={vm.id}>{vm.id.substring(0, 12)}</div>
              </div>
              <div className="col-span-3 text-sm text-slate-400 flex items-center gap-2"><HardDrive size={14} /> {vm.labels['image'] || 'alpine'}</div>
              <div className="col-span-2 text-sm text-slate-400 font-mono">{vm.ip_address}</div>
              <div className="col-span-2 flex justify-end gap-2 opacity-60 group-hover:opacity-100 transition">
                <button className="p-1.5 hover:bg-slate-700 rounded text-slate-400 hover:text-white"><Terminal size={16} /></button>
                <button className="p-1.5 hover:bg-red-500/20 rounded text-slate-400 hover:text-red-400"><Square size={16} /></button>
              </div>
            </div>
          ))}
          {!loading && vms.length === 0 && <div className="p-12 text-center text-slate-500">No MicroVMs running.</div>}
        </div>
      </div>
    </div>
  )
}

function ImagesView() {
  const [images, setImages] = useState<string[]>([]);
  useEffect(() => {
    fetch(`${API_BASE}/images`).then(r => r.json()).then(setImages).catch(console.error);
  }, []);
  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Images</h2>
      <div className="bg-slate-900 rounded-xl border border-slate-800 overflow-hidden">
        <div className="grid grid-cols-3 gap-4 p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase">
          <div>Repository</div>
          <div>Tag</div>
          <div className="text-right">Size</div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {images.map((img, i) => (
            <div key={i} className="grid grid-cols-3 gap-4 p-4 text-sm text-slate-300">
              <div>{img}</div>
              <div>latest</div>
              <div className="text-right text-slate-500 text-mono">--</div>
            </div>
          ))}
          {images.length === 0 && <div className="p-8 text-center text-slate-500">No images cached locally.</div>}
        </div>
      </div>
    </div>
  )
}

function VolumesView() {
  const [vols, setVols] = useState<Volume[]>([]);
  useEffect(() => {
    fetch(`${API_BASE}/volumes`).then(r => r.json()).then(setVols).catch(console.error);
  }, []);
  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Volumes</h2>
      <div className="bg-slate-900 rounded-xl border border-slate-800 overflow-hidden">
        <div className="p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase">Volume Name / Path</div>
        <div className="divide-y divide-slate-800/50">
          {vols.map((v, i) => (
            <div key={i} className="p-4 text-sm text-slate-300">
              <div className="font-medium">{v.name}</div>
              <div className="text-xs text-slate-500 font-mono mt-1">{v.path}</div>
            </div>
          ))}
          {vols.length === 0 && <div className="p-8 text-center text-slate-500">No volumes found.</div>}
        </div>
      </div>
    </div>
  )
}

function NetworksView() {
  const [nets, setNets] = useState<Network[]>([]);
  useEffect(() => {
    fetch(`${API_BASE}/networks`).then(r => r.json()).then(setNets).catch(console.error);
  }, []);
  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Networks</h2>
      <div className="bg-slate-900 rounded-xl border border-slate-800 overflow-hidden">
        <div className="grid grid-cols-2 gap-4 p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase">
          <div>Network Name</div>
          <div>Subnet</div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {nets.map((n, i) => (
            <div key={i} className="grid grid-cols-2 gap-4 p-4 text-sm text-slate-300">
              <div className="flex items-center gap-2"><Globe size={14} className="text-blue-400" /> {n.name}</div>
              <div className="font-mono text-slate-500">{n.subnet}</div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}

function StatsView() {
  const [nodes, setNodes] = useState<any[]>([]);
  useEffect(() => {
    fetch(`${API_BASE}/swarm/nodes`).then(r => r.json()).then(setNodes).catch(console.error);
  }, []);
  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Cluster Stats</h2>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {nodes.map((n, i) => (
          <div key={i} className="bg-slate-900 border border-slate-800 p-6 rounded-xl flex flex-col gap-4">
            <div className="flex items-center gap-3 mb-2">
              <Server className="text-orange-500" />
              <div>
                <h3 className="font-bold text-white">{n.hostname}</h3>
                <div className="text-xs text-slate-500">{n.role}</div>
              </div>
            </div>
            <div className="space-y-2">
              <div className="flex justify-between text-sm"><span className="text-slate-500">IP</span> <span className="font-mono text-slate-300">{n.ip}</span></div>
              <div className="flex justify-between text-sm"><span className="text-slate-500">CPU</span> <span className="text-slate-300">{(n.resources?.cpu_usage || 0).toFixed(1)}%</span></div>
              <div className="flex justify-between text-sm"><span className="text-slate-500">Mem</span> <span className="text-slate-300">{(n.resources?.memory_usage_mb || 0)} MB</span></div>
            </div>
          </div>
        ))}
        {nodes.length === 0 && <div className="col-span-3 text-center text-slate-500">No cluster nodes connected.</div>}
      </div>
    </div>
  )
}

export default App
