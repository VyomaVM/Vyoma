import { useState } from 'react';
import { Search, Plus } from 'lucide-react';
import { Button } from '../components/ui';

interface HubImage {
  name: string;
  description: string;
  stars: number;
  pulls: string;
}

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

export function HubBrowserView() {
  const [query, setQuery] = useState('');
  const [images, setImages] = useState<HubImage[]>([]);
  const [loading, setLoading] = useState(false);

  const searchHub = async () => {
    if (!query.trim()) return;
    setLoading(true);
    try {
      const res = await fetch(`${API_BASE}/hub/search?q=${encodeURIComponent(query)}`);
      const data = await res.json();
      setImages(data.images || []);
    } catch {
      setImages([]);
    }
    setLoading(false);
  };

  const pullImage = async (name: string) => {
    await fetch(`${API_BASE}/pull`, {
      method: 'POST',
      body: JSON.stringify({ image: name }),
    });
    alert(`Pulling ${name}...`);
  };

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="mb-8">
        <h2 className="text-2xl font-bold text-white mb-1">Hub Browser</h2>
        <p className="text-sm text-slate-400">Search images from Ignite Hub or Docker Hub.</p>
      </header>

      <div className="flex gap-3 mb-6">
        <div className="flex-1 relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" size={18} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && searchHub()}
            placeholder="Search images..."
            className="w-full bg-slate-900 border border-slate-700 rounded-lg pl-10 pr-4 py-2.5 text-white placeholder-slate-500 focus:border-orange-500 focus:outline-none"
          />
        </div>
        <Button onClick={searchHub}>Search</Button>
      </div>

      {loading ? (
        <div className="text-center py-12 text-slate-500 animate-pulse">Searching Hub...</div>
      ) : images.length === 0 ? (
        <div className="text-center py-12 text-slate-500">Search for images to browse the Hub.</div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {images.map((img, i) => (
            <div
              key={i}
              className="bg-slate-900 border border-slate-800 rounded-xl p-4 hover:border-orange-500/30 transition"
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <h3 className="font-semibold text-white">{img.name}</h3>
                  <p className="text-sm text-slate-500 mt-1 line-clamp-2">{img.description || 'No description'}</p>
                </div>
                <button
                  onClick={() => pullImage(img.name)}
                  className="p-2 bg-slate-800 hover:bg-orange-600 rounded-lg text-slate-400 hover:text-white transition ml-3"
                >
                  <Plus size={16} />
                </button>
              </div>
              <div className="flex gap-4 mt-3 text-xs text-slate-500">
                <span>★ {img.stars}</span>
                <span>↓ {img.pulls}</span>
             Phase 6.5 involves creating an in-VM binary that operates from within each MicroVM rather than on the host system. It establishes vsock communication at port 9999 to gather process information, execute commands, retrieve metrics, and read files—all functionality that complements what we're building. Let me get started on implementing this.
</think>


<minimax:tool_call>
<invoke name="write">
<parameter name="content">import { HardDrive, Database, Globe, Settings, Activity, Server, Search } from 'lucide-react';
import { useImages, useVolumes, useNetworks, useSwarmNodes } from '../hooks/useApi';
import { Card, EmptyState, Loading } from '../components/ui';

export function ImagesView() {
  const { data, loading } = useImages();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Images</h2>
      <Card>
        <div className="grid grid-cols-3 gap-4 p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase">
          <div>Repository</div>
          <div>Tag</div>
          <div className="text-right">Size</div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {loading ? (
            <Loading text="Loading images..." />
          ) : !data?.length ? (
            <EmptyState title="No images" description="Pull an image to get started." icon={<HardDrive size={48} />} />
          ) : (
            data.map((img, i) => (
              <div key={i} className="grid grid-cols-3 gap-4 p-4 text-sm text-slate-300">
                <div>{img}</div>
                <div>latest</div>
                <div className="text-right text-slate-500 text-mono">--</div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}

export function VolumesView() {
  const { data, loading } = useVolumes();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Volumes</h2>
      <Card>
        <div className="p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase">Volume Name / Path</div>
        <div className="divide-y divide-slate-800/50">
          {loading ? (
            <Loading text="Loading volumes..." />
          ) : !data?.length ? (
            <EmptyState title="No volumes" description="Create a volume to get started." icon={<Database size={48} />} />
          ) : (
            data.map((v, i) => (
              <div key={i} className="p-4 text-sm text-slate-300">
                <div className="font-medium">{v.name}</div>
                <div className="text-xs text-slate-500 font-mono mt-1">{v.path}</div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}

export function NetworksView() {
  const { data, loading } = useNetworks();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Networks</h2>
      <Card>
        <div className="grid grid-cols-2 gap-4 p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase">
          <div>Network Name</div>
          <div>Subnet</div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {loading ? (
            <Loading text="Loading networks..." />
          ) : !data?.length ? (
            <EmptyState title="No networks" description="Create a network to get started." icon={<Globe size={48} />} />
          ) : (
            data.map((n, i) => (
              <div key={i} className="grid grid-cols-2 gap-4 p-4 text-sm text-slate-300">
                <div className="flex items-center gap-2">
                  <Globe size={14} className="text-blue-400" /> {n.name}
                </div>
                <div className="font-mono text-slate-500">{n.subnet}</div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}

export function StatsView() {
  const { data, loading } = useSwarmNodes();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-white mb-6">Cluster Stats</h2>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {loading ? (
          <Loading text="Loading cluster stats..." />
        ) : !data?.length ? (
          <EmptyState title="No cluster nodes" description="Join a swarm to see cluster stats." icon={<Server size={48} />} />
        ) : (
          data.map((n, i) => (
            <Card key={i} hover>
              <div className="flex items-center gap-3 mb-2">
                <Server className="text-orange-500" />
                <div>
                  <h3 className="font-bold text-white">{n.hostname}</h3>
                  <div className="text-xs text-slate-500">{n.role}</div>
                </div>
              </div>
              <div className="space-y-2">
                <div className="flex justify-between text-sm">
                  <span className="text-slate-500">IP</span>
                  <span className="font-mono text-slate-300">{n.ip}</span>
                </div>
                <div className="flex justify-between text-sm">
                  <span className="text-slate-500">CPU</span>
                  <span className="text-slate-300">{(n.resources?.cpu_usage || 0).toFixed(1)}%</span>
                </div>
                <div className="flex justify-between text-sm">
                  <span className="text-slate-500">Mem</span>
                  <span className="text-slate-300">{(n.resources?.memory_usage_mb || 0)} MB</span>
                </div>
              </div>
            </Card>
          ))
        )}
      </div>
    </div>
  );
}

export function SettingsView() {
  return (
    <EmptyState title="Settings" description="Settings configuration coming soon." icon={<Settings size={48} />} />
  );
}
