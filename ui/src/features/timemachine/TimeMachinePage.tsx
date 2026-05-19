import { useState } from 'react';
import { Clock, Play, Trash2, Plus } from 'lucide-react';
import { useVmList } from '../../hooks/queries/useVmList';
import { useSnapshots, useCreateSnapshot, useRestoreSnapshot } from '../../hooks/queries/useSnapshots';
import { Button, EmptyState, Loading, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '../../components/ui';

export function TimeMachinePage() {
  const { data: vmsData } = useVmList();
  const [selectedVm, setSelectedVm] = useState<string>('');
  
  const { data: snapshotData, isLoading } = useSnapshots(selectedVm);
  const createSnapshot = useCreateSnapshot();
  const restoreSnapshot = useRestoreSnapshot();

  const snapshots = snapshotData?.snapshots || [];

  const formatTime = (ts: number) => new Date(ts * 1000).toLocaleString();
  const formatSize = (b: number) => (b < 1024 * 1024 ? `${(b / 1024).toFixed(1)}KB` : `${(b / 1024 / 1024).toFixed(1)}MB`);

  const handleRestore = (snapId: string) => {
    if (!confirm(`Restore to snapshot ${snapId.slice(0, 8)}?`)) return;
    restoreSnapshot.mutate({ vmId: selectedVm, snapshotId: snapId });
  };

  const handleCreateSnapshot = () => {
    if (!selectedVm) return;
    const name = prompt('Snapshot name (optional):');
    createSnapshot.mutate({ vmId: selectedVm, name: name || undefined });
  };

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="mb-8">
        <h2 className="text-2xl font-bold text-foreground mb-1">TimeMachine</h2>
        <p className="text-sm text-muted-foreground">View and restore VM snapshots in a timeline.</p>
      </header>

      <div className="flex gap-4 mb-6">
        <Select value={selectedVm} onValueChange={setSelectedVm}>
          <SelectTrigger className="w-[280px]">
            <SelectValue placeholder="Select a VM..." />
          </SelectTrigger>
          <SelectContent>
            {vmsData?.vms?.map((v) => (
              <SelectItem key={v.id} value={v.id}>
                {v.labels['vyoma.service'] || v.id.slice(0, 12)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button onClick={handleCreateSnapshot} disabled={!selectedVm || createSnapshot.isPending}>
          <Plus size={16} className="mr-2" /> Create Snapshot
        </Button>
      </div>

      {!selectedVm ? (
        <EmptyState title="Select a VM" description="Choose a VM to view its snapshot timeline." icon={<Clock size={48} />} />
      ) : isLoading ? (
        <Loading text="Loading snapshots..." />
      ) : snapshots.length === 0 ? (
        <EmptyState title="No snapshots" description="Create a snapshot to start TimeMachine." icon={<Clock size={48} />} />
      ) : (
        <div className="relative">
          <div className="absolute left-8 top-0 bottom-0 w-0.5 bg-border" />
          <div className="space-y-4">
            {snapshots.map((snap, i) => (
              <div key={snap.id} className="relative flex items-start gap-6 ml-4">
                <div className="absolute left-4 w-8 h-8 -ml-4 rounded-full bg-card border-2 border-primary flex items-center justify-center z-10">
                  <Clock size={14} className="text-primary" />
                </div>
                <div className="flex-1 ml-8 bg-card border border-border rounded-xl p-4 hover:border-primary/50 transition">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="font-semibold text-foreground">snap:{snapshots.length - i - 1}</div>
                      <div className="text-xs text-muted-foreground mt-1">
                        {formatTime(snap.created_at)} · {formatSize(snap.size_bytes)}
                      </div>
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={() => handleRestore(snap.id)}
                        className="p-2 hover:bg-muted rounded text-muted-foreground hover:text-foreground"
                        title="Restore"
                        disabled={restoreSnapshot.isPending}
                      >
                        <Play size={16} />
                      </button>
                      <button className="p-2 hover:bg-destructive/20 rounded text-muted-foreground hover:text-destructive" title="Delete">
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </div>
                  {snap.label && <div className="mt-2 text-sm text-muted-foreground">{snap.label}</div>}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
