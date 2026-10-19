import { useState } from 'react';
import { RefreshCw, Square, Play, Pause, HardDrive, Send, XCircle, Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import { useVmList, useVmMutations } from '../../hooks/queries/useVmList';
import type { Vm } from '../../hooks/queries/useVmList';
import { 
  Card, 
  Button, 
  StatusBadge, 
  EmptyState, 
  Skeleton, 
  Dialog, 
  DialogContent, 
  DialogHeader, 
  DialogTitle, 
  Input 
} from '../../components/ui';
import { useVmStats } from '../../hooks/queries/useVmStats';

export function VmsPage() {
  const { data, isLoading, refetch } = useVmList();
  const [selectedResizeVm, setSelectedResizeVm] = useState<Vm | null>(null);
  const [selectedTeleportVm, setSelectedTeleportVm] = useState<Vm | null>(null);

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="flex items-center justify-between mb-8">
        <div>
          <h2 className="text-2xl font-bold text-foreground mb-1">MicroVMs</h2>
          <p className="text-sm text-muted-foreground">Manage your running micro-virtual machines with Cloud Hypervisor v54.</p>
        </div>
        <button
          onClick={() => refetch()}
          className="p-2 bg-card border border-border rounded hover:bg-muted transition text-muted-foreground hover:text-foreground"
        >
          <RefreshCw size={18} />
        </button>
      </header>

      <Card>
        <div className="grid grid-cols-12 gap-4 p-4 border-b border-border text-xs font-semibold text-muted-foreground uppercase tracking-wider bg-card">
          <div className="col-span-1 flex justify-center">Status</div>
          <div className="col-span-3">Name / ID</div>
          <div className="col-span-2">Image</div>
          <div className="col-span-2">IP Address</div>
          <div className="col-span-2">Usage</div>
          <div className="col-span-2 text-right">Actions</div>
        </div>
        <div className="divide-y divide-border/50 bg-card">
          {isLoading ? (
            Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="grid grid-cols-12 gap-4 p-4 items-center">
                <div className="col-span-1 flex justify-center"><Skeleton className="h-6 w-16 rounded-full" /></div>
                <div className="col-span-3 space-y-2">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-24" />
                </div>
                <div className="col-span-2"><Skeleton className="h-4 w-20" /></div>
                <div className="col-span-2"><Skeleton className="h-4 w-24" /></div>
                <div className="col-span-2"><Skeleton className="h-4 w-32" /></div>
                <div className="col-span-2 flex justify-end gap-2">
                  <Skeleton className="h-8 w-8 rounded" />
                </div>
              </div>
            ))
          ) : !data?.vms?.length ? (
            <EmptyState title="No MicroVMs running" />
          ) : (
            data.vms.map((vm) => (
              <VmRow 
                key={vm.id} 
                vm={vm} 
                onResize={(v) => setSelectedResizeVm(v)}
                onTeleport={(v) => setSelectedTeleportVm(v)}
              />
            ))
          )}
        </div>
      </Card>

      <ResizeDiskDialog
        vm={selectedResizeVm}
        onClose={() => setSelectedResizeVm(null)}
      />

      <TeleportDialog
        vm={selectedTeleportVm}
        onClose={() => setSelectedTeleportVm(null)}
      />
    </div>
  );
}

export function VmRow({ 
  vm, 
  onResize, 
  onTeleport 
}: { 
  vm: Vm; 
  onResize: (vm: Vm) => void; 
  onTeleport: (vm: Vm) => void; 
}) {
  const { startVm, stopVm, pauseVm, resumeVm } = useVmMutations();
  const isRunning = vm.status?.startsWith('Running');
  const isPaused = vm.status?.startsWith('Paused');

  const { data: stats } = useVmStats(vm.id, isRunning);

  const formatBytes = (bytes: number) => {
    if (!bytes) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  return (
    <div className="grid grid-cols-12 gap-4 p-4 items-center group hover:bg-muted/30 transition">
      <div className="col-span-1 flex justify-center">
        <StatusBadge 
          status={
            vm.status?.startsWith('Running') ? 'running' :
            vm.status?.startsWith('AttestationFailed') ? 'attestation_failed' :
            vm.status?.startsWith('PendingAttestation') ? 'pending_attestation' :
            vm.status?.startsWith('Error') ? 'error' :
            vm.status?.startsWith('Paused') ? 'paused' : 'stopped'
          } 
        />
      </div>
      <div className="col-span-3">
        <div className="font-semibold text-foreground text-sm">
          {vm.labels['vyoma.service'] || 'MicroVM'}
        </div>
        <div className="text-xs text-muted-foreground font-mono mt-0.5 truncate" title={vm.id}>
          {vm.id.substring(0, 12)}
        </div>
      </div>
      <div className="col-span-2 text-sm text-muted-foreground">
        {vm.labels['image'] || 'alpine'}
      </div>
      <div className="col-span-2 text-sm text-muted-foreground font-mono">
        {vm.ip_address || '--'}
      </div>
      <div className="col-span-2 flex flex-col gap-1 justify-center text-xs text-muted-foreground">
        {isRunning && stats ? (
          <>
            <div><span className="font-medium text-foreground">{stats.cpu_percent.toFixed(1)}%</span> CPU</div>
            <div><span className="font-medium text-foreground">{formatBytes(stats.memory_usage_bytes)}</span> RAM</div>
          </>
        ) : (
          <span className="opacity-50">--</span>
        )}
      </div>
      <div className="col-span-2 flex justify-end gap-1.5 opacity-70 group-hover:opacity-100 transition">
        {isRunning && (
          <>
            <button 
              onClick={() => onResize(vm)}
              className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-cyan-400"
              title="Resize Virtual Disk (Cloud Hypervisor v54)"
            >
              <HardDrive size={16} />
            </button>
            <button 
              onClick={() => onTeleport(vm)}
              className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-purple-400"
              title="Live Teleport / Migrate with Cancel Option"
            >
              <Send size={16} />
            </button>
          </>
        )}

        {isRunning ? (
          <button 
            onClick={() => pauseVm.mutate(vm.id)}
            className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-yellow-400"
            title="Pause"
          >
            <Pause size={16} />
          </button>
        ) : isPaused ? (
          <button 
            onClick={() => resumeVm.mutate(vm.id)}
            className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-green-400"
            title="Resume"
          >
            <Play size={16} />
          </button>
        ) : (
          <button 
            onClick={() => startVm.mutate(vm.id)}
            className="p-1.5 hover:bg-muted rounded text-muted-foreground hover:text-green-400"
            title="Start"
          >
            <Play size={16} />
          </button>
        )}
        
        <button 
          onClick={() => stopVm.mutate(vm.id)}
          className="p-1.5 hover:bg-destructive/20 rounded text-muted-foreground hover:text-destructive"
          title="Stop"
        >
          <Square size={16} />
        </button>
      </div>
    </div>
  );
}

function ResizeDiskDialog({ vm, onClose }: { vm: Vm | null; onClose: () => void }) {
  const [diskId, setDiskId] = useState('rootfs');
  const [sizeGb, setSizeGb] = useState('10');
  const { resizeDisk } = useVmMutations();

  if (!vm) return null;

  const handleResize = async (e: React.FormEvent) => {
    e.preventDefault();
    const gb = parseFloat(sizeGb);
    if (isNaN(gb) || gb <= 0) {
      toast.error('Please enter a valid positive number for disk size');
      return;
    }
    const desired_size = Math.round(gb * 1024 * 1024 * 1024);
    try {
      const res = await resizeDisk.mutateAsync({
        id: vm.id,
        disk_id: diskId.trim() || 'rootfs',
        desired_size,
      });
      toast.success(res.message || 'Disk resized successfully');
      onClose();
    } catch (err: any) {
      toast.error(err.message || 'Failed to resize disk');
    }
  };

  return (
    <Dialog open={!!vm} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <HardDrive className="text-primary w-5 h-5" />
            Resize Virtual Disk
          </DialogTitle>
        </DialogHeader>
        <form onSubmit={handleResize} className="space-y-4 pt-2">
          <div>
            <label className="text-xs font-semibold text-muted-foreground uppercase tracking-wider block mb-1">
              MicroVM
            </label>
            <div className="font-mono text-sm text-foreground bg-muted p-2 rounded">
              {vm.labels['vyoma.service'] || vm.id} ({vm.id.slice(0, 12)})
            </div>
          </div>
          <div>
            <label className="text-xs font-semibold text-muted-foreground uppercase tracking-wider block mb-1">
              Disk Identifier
            </label>
            <Input
              value={diskId}
              onChange={(e) => setDiskId(e.target.value)}
              placeholder="rootfs"
              required
            />
            <p className="text-xs text-muted-foreground mt-1">
              The internal block device identifier registered in Cloud Hypervisor (defaults to "rootfs").
            </p>
          </div>
          <div>
            <label className="text-xs font-semibold text-muted-foreground uppercase tracking-wider block mb-1">
              Desired Total Capacity (GB)
            </label>
            <Input
              type="number"
              min="1"
              step="0.5"
              value={sizeGb}
              onChange={(e) => setSizeGb(e.target.value)}
              placeholder="10"
              required
            />
            <p className="text-xs text-muted-foreground mt-1">
              Total desired disk size in Gigabytes ({sizeGb ? (parseFloat(sizeGb) * 1024 * 1024 * 1024).toLocaleString() : 0} bytes).
            </p>
          </div>
          <div className="p-3 bg-primary/10 border border-primary/20 rounded text-xs text-muted-foreground">
            <strong>Note:</strong> Cloud Hypervisor v54.0 online dynamic resizing expands standalone disk images live without guest downtime.
          </div>
          <div className="flex justify-end gap-2 pt-2">
            <Button type="button" variant="outline" onClick={onClose} disabled={resizeDisk.isPending}>
              Cancel
            </Button>
            <Button type="submit" disabled={resizeDisk.isPending}>
              {resizeDisk.isPending && <Loader2 className="w-4 h-4 mr-1 animate-spin" />}
              Apply Resize
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function TeleportDialog({ vm, onClose }: { vm: Vm | null; onClose: () => void }) {
  const [targetIp, setTargetIp] = useState('');
  const [bandwidthMbps, setBandwidthMbps] = useState('');
  const [activeSession, setActiveSession] = useState<{ sessionId: string; status: string } | null>(null);
  const { teleportVm, cancelTeleport } = useVmMutations();

  if (!vm) return null;

  const handleTeleport = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!targetIp.trim()) {
      toast.error('Please enter the target node IP or hostname');
      return;
    }
    const bw = bandwidthMbps ? parseInt(bandwidthMbps, 10) : undefined;
    try {
      const res = await teleportVm.mutateAsync({
        id: vm.id,
        target_node_ip: targetIp.trim(),
        bandwidth_mbps: bw,
      });
      setActiveSession({
        sessionId: res.session_id,
        status: res.status || 'initiating',
      });
      toast.success(res.message || 'Live migration initiated');
    } catch (err: any) {
      toast.error(err.message || 'Failed to start teleportation');
    }
  };

  const handleCancelMigration = async () => {
    try {
      const targetId = activeSession?.sessionId || vm.id;
      const res = await cancelTeleport.mutateAsync(targetId);
      if (activeSession) {
        setActiveSession({ ...activeSession, status: 'cancelled' });
      }
      toast.success(res.message || 'Migration cancelled successfully');
    } catch (err: any) {
      toast.error(err.message || 'Failed to cancel migration');
    }
  };

  return (
    <Dialog open={!!vm} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Send className="text-primary w-5 h-5" />
            Live Teleportation / Migration
          </DialogTitle>
        </DialogHeader>

        {activeSession ? (
          <div className="space-y-4 pt-2">
            <div className="p-4 bg-muted/60 border border-border rounded-lg space-y-3">
              <div className="flex justify-between items-center">
                <span className="text-xs uppercase font-semibold text-muted-foreground">Migration Session</span>
                <span className="font-mono text-xs text-foreground">{activeSession.sessionId.slice(0, 12)}...</span>
              </div>
              <div className="flex justify-between items-center">
                <span className="text-xs uppercase font-semibold text-muted-foreground">Current Status</span>
                <span className="px-2 py-0.5 rounded text-xs font-semibold uppercase bg-primary/20 text-primary border border-primary/30">
                  {activeSession.status}
                </span>
              </div>
            </div>

            {activeSession.status !== 'cancelled' && activeSession.status !== 'completed' && (
              <div className="p-3 bg-destructive/10 border border-destructive/20 rounded-md flex items-center justify-between">
                <div>
                  <div className="text-sm font-medium text-destructive">Active Live Migration</div>
                  <div className="text-xs text-muted-foreground">Cloud Hypervisor v54 native migration cancel</div>
                </div>
                <Button
                  variant="destructive"
                  size="sm"
                  onClick={handleCancelMigration}
                  disabled={cancelTeleport.isPending}
                >
                  {cancelTeleport.isPending ? (
                    <Loader2 className="w-4 h-4 mr-1 animate-spin" />
                  ) : (
                    <XCircle className="w-4 h-4 mr-1" />
                  )}
                  Cancel Migration
                </Button>
              </div>
            )}

            <div className="flex justify-end pt-2">
              <Button variant="outline" onClick={onClose}>
                Close
              </Button>
            </div>
          </div>
        ) : (
          <form onSubmit={handleTeleport} className="space-y-4 pt-2">
            <div>
              <label className="text-xs font-semibold text-muted-foreground uppercase tracking-wider block mb-1">
                Source MicroVM
              </label>
              <div className="font-mono text-sm text-foreground bg-muted p-2 rounded">
                {vm.labels['vyoma.service'] || vm.id} ({vm.id.slice(0, 12)})
              </div>
            </div>
            <div>
              <label className="text-xs font-semibold text-muted-foreground uppercase tracking-wider block mb-1">
                Target Node IP / Address
              </label>
              <Input
                value={targetIp}
                onChange={(e) => setTargetIp(e.target.value)}
                placeholder="192.168.1.100 or node2.vyoma.internal"
                required
              />
            </div>
            <div>
              <label className="text-xs font-semibold text-muted-foreground uppercase tracking-wider block mb-1">
                Bandwidth Limit (Mbps, Optional)
              </label>
              <Input
                type="number"
                min="10"
                value={bandwidthMbps}
                onChange={(e) => setBandwidthMbps(e.target.value)}
                placeholder="Unlimited (or e.g. 1000)"
              />
            </div>
            <div className="p-3 bg-muted/60 border border-border rounded text-xs text-muted-foreground">
              Memory pages are migrated iteratively via Cloud Hypervisor v54. Live migration cancellation is supported at any time during transfer.
            </div>
            <div className="flex justify-end gap-2 pt-2">
              <Button type="button" variant="outline" onClick={onClose} disabled={teleportVm.isPending}>
                Cancel
              </Button>
              <Button type="submit" disabled={teleportVm.isPending}>
                {teleportVm.isPending && <Loader2 className="w-4 h-4 mr-1 animate-spin" />}
                Initiate Teleport
              </Button>
            </div>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
