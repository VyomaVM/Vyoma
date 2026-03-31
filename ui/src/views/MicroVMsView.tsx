import { RefreshCw, Terminal, Square } from 'lucide-react';
import { useVmList } from '../hooks/useApi';
import { Card, StatusBadge, EmptyState, Loading } from './ui';

export function MicroVMsView() {
  const { data, loading, refetch } = useVmList();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="flex items-center justify-between mb-8">
        <div>
          <h2 className="text-2xl font-bold text-white mb-1">MicroVMs</h2>
          <p className="text-sm text-slate-400">Manage your running micro-virtual machines.</p>
        </div>
        <button
          onClick={refetch}
          className="p-2 bg-slate-800 border border-slate-700 rounded hover:bg-slate-700 transition text-slate-400 hover:text-white"
        >
          <RefreshCw size={18} />
        </button>
      </header>

      <Card>
        <div className="grid grid-cols-12 gap-4 p-4 border-b border-slate-800 text-xs font-semibold text-slate-500 uppercase tracking-wider">
          <div className="col-span-1 flex justify-center">Status</div>
          <div className="col-span-4">Name / ID</div>
          <div className="col-span-3">Image</div>
          <div className="col-span-2">IP Address</div>
          <div className="col-span-2 text-right">Actions</div>
        </div>
        <div className="divide-y divide-slate-800/50">
          {loading ? (
            <Loading text="Loading VMs..." />
          ) : !data?.vms.length ? (
            <EmptyState title="No MicroVMs running" />
          ) : (
            data.vms.map((vm) => (
              <div
                key={vm.id}
                className="grid grid-cols-12 gap-4 p-4 items-center group hover:bg-slate-800/30 transition"
              >
                <div className="col-span-1 flex justify-center">
                  <StatusBadge status={vm.status === 'Running' ? 'running' : 'stopped'} />
                </div>
                <div className="col-span-4">
                  <div className="font-semibold text-white text-sm">
                    {vm.labels['ignite.service'] || 'MicroVM'}
                  </div>
                  <div className="text-xs text-slate-500 font-mono mt-0.5 truncate" title={vm.id}>
                    {vm.id.substring(0, 12)}
                  </div>
                </div>
                <div className="col-span-3 text-sm text-slate-400">
                  {vm.labels['image'] || 'alpine'}
                </div>
                <div className="col-span-2 text-sm text-slate-400 font-mono">
                  {vm.ip_address}
                </div>
                <div className="col-span-2 flex justify-end gap-2 opacity-60 group-hover:opacity-100 transition">
                  <button className="p-1.5 hover:bg-slate-700 rounded text-slate-400 hover:text-white">
                    <Terminal size={16} />
                  </button>
                  <button className="p-1.5 hover:bg-red-500/20 rounded text-slate-400 hover:text-red-400">
                    <Square size={16} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
