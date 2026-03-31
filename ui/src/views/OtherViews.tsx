import { HardDrive, Database, Globe, Settings, Server } from 'lucide-react';
import { useImages, useVolumes, useNetworks, useSwarmNodes, type Network } from '../hooks/useApi';
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
          ) : !data?.networks?.length ? (
            <EmptyState title="No networks" description="Create a network to get started." icon={<Globe size={48} />} />
          ) : (
            data.networks.map((n: Network, i: number) => (
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
