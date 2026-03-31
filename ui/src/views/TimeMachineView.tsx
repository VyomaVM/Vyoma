import { useState, useEffect } from 'react';
import { Clock, Play, Trash2, Plus } from 'lucide-react';
import { useVmList, useSnapshots } from '../hooks/useApi';
import { Card, Button, EmptyState, Loading } from '../components/ui';

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

export function TimeMachineView() {
  const { data: vmsData } = useVmList();
  const [selectedVm, setSelectedVm] = useState('');
  const { data: snapshotData, loading, refetch } = useSnapshots(selectedVm);

  const snapshots = snapshotData?.snapshots || [];

  const formatTime = (ts: number) => new Date(ts * 1000).toLocaleString();
  const formatSize = (b: number) => (b < 1024 * 1024 ? `${(b / 1024).toFixed(1)}KB` : `${(b / 1024 / 1024).toFixed(1)}MB`);

  const handleRestore = async (snapId: string) => {
    if (!confirm(`Restore to snapshot ${snapId.slice(0, 8)}?`)) return;
    await fetch(`${API_BASE}/snapshots/${selectedVm}/restore`, {
      method: 'POST',
      body: JSON.stringify({ snapshot_id: snapId }),
    });
    alert('VM restored!');
  };

  const handleCreateSnapshot = async () => {
    if (!selectedVm) return;
    const name = prompt('Snapshot name (optional):');
    await fetch(`${API_BASE}/snapshots/${selectedVm}`, {
      method: 'POST',
      body: JSON.stringify({ name: name || '' }),
    });
    refetch();
  };

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="mb-8">
        <h2 className="text-2xl font-bold text-white mb-1">TimeMachine</h2>
        <p className="text-sm text-slate-400">View and restore VM snapshots in a timeline.</p>
      </header>

      <div className="flex gap-4 mb-6">
        <select
          value={selectedVm}
          onChange={(e) => setSelectedVm(e.target.value)}
          className="bg-slate-900 border border-slate-700 rounded-lg px-4 py-2 text-white min-w-[200px]"
        >
          <option value="">Select a VM...</option>
          {vmsData?.vms.map((v) => (
            <option key={v.id} value={v.id}>
              {v.labels['ignite.service'] || v.id.slice(0, 12)}
            </option>
          ))}
        </select>
        <Button onClick={handleCreateSnapshot} disabled={!selectedVm}>
          <Plus size={16} /> Create Snapshot
        </Button>
      </div>

      {!selectedVm ? (
        <EmptyState title="Select a VM" description="Choose a VM to view its snapshot timeline." icon={<Clock size={48} />} />
      ) : loading ? (
        <Loading text="Loading snapshots..." />
      ) : snapshots.length === 0 ? (
        <EmptyState title="No snapshots" description="Create a snapshot to start TimeMachine." icon={<Clock size={48} />} />
      ) : (
        <div className="relative">
          <div className="absolute left-8 top-0 bottom-0 w-0.5 bg-slate-800" />
          <div className="space-y-4">
            {snapshots.map((snap, i) => (
              <div key={snap.id} className="relative flex items-start gap-6 ml-4">
                <div className="absolute left-4 w-8 h-8 -ml-4 rounded-full bg-slate-800 border-2 border-orange-500 flex items-center justify-center z-10">
                  <Clock size={14} className="text-orange-500" />
                </div>
                <div className="flex-1 ml-8 bg-slate-900 border border-slate-800 rounded-xl p-4 hover:border-orange-500/30 transition">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="font-semibold text-white">snap:{snapshots.length - i - 1}</div>
                      <div className="text-xs text-slate-500 mt-1">
                        {formatTime(snap.created_at)} · {formatSize(snap.size_bytes)}
                      </div>
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={() => handleRestore(snap.id)}
                        className="p-2 hover:bg-slate-800 rounded text-slate-400 hover:text-white"
                        title="Restore"
                      >
                        <Play size={16} />
                      </button>
                      <button className="p-2 hover:bg-red-500/20 rounded text-slate-400 hover:text-red-400" title="Delete">
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                  {snap.label && <div className="mt-2 text-sm text-slate-400">{snap.label}</div>}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
