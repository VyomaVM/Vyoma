import { useState } from 'react';
import { ShieldCheck } from 'lucide-react';
import { useVmList } from '../../hooks/queries/useVmList';
import { Card, Button, StatusBadge, Dialog, DialogContent, DialogHeader, DialogTitle, Skeleton } from '../../components/ui';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { apiFetch } from '../../lib/api-client';

export function AttestationPage() {
  const { data: vmsData, isLoading } = useVmList();
  const queryClient = useQueryClient();
  const [selectedResult, setSelectedResult] = useState<any>(null);

  const attestMutation = useMutation({
    mutationFn: (id: string) => apiFetch(`/attest/${id}`, { method: 'POST' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['vms'] });
    }
  });

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="mb-8">
        <h2 className="text-2xl font-bold text-foreground mb-1 flex items-center gap-2">
          <ShieldCheck className="text-primary" /> Attestation
        </h2>
        <p className="text-sm text-muted-foreground">Verify the integrity of running MicroVMs via hardware root of trust.</p>
      </header>

      <Card>
        <div className="grid grid-cols-12 gap-4 p-4 border-b border-border text-xs font-semibold text-muted-foreground uppercase bg-card">
          <div className="col-span-3">VM Name</div>
          <div className="col-span-3">Image</div>
          <div className="col-span-3">Status</div>
          <div className="col-span-3 text-right">Actions</div>
        </div>
        <div className="divide-y divide-border/50 bg-card">
          {isLoading ? (
            Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="grid grid-cols-12 gap-4 p-4 items-center">
                <div className="col-span-3"><Skeleton className="h-4 w-32" /></div>
                <div className="col-span-3"><Skeleton className="h-4 w-20" /></div>
                <div className="col-span-3"><Skeleton className="h-6 w-24 rounded-full" /></div>
                <div className="col-span-3 flex justify-end gap-2">
                  <Skeleton className="h-8 w-24 rounded" />
                  <Skeleton className="h-8 w-20 rounded" />
                </div>
              </div>
            ))
          ) : (
            vmsData?.vms?.map(vm => {
              const isRunning = vm.status?.startsWith('Running');
              const statusStr = vm.status || '';
              
              let attestationState = 'unknown';
              if (statusStr.includes('AttestationFailed')) attestationState = 'attestation_failed';
              else if (statusStr.includes('PendingAttestation')) attestationState = 'pending_attestation';
              else if (isRunning) attestationState = 'running';
              else attestationState = 'stopped';

              return (
                <div key={vm.id} className="grid grid-cols-12 gap-4 p-4 items-center">
                  <div className="col-span-3 font-medium text-foreground">
                    {vm.labels['vyoma.service'] || vm.id.slice(0, 12)}
                  </div>
                  <div className="col-span-3 text-muted-foreground text-sm">
                    {vm.labels['image'] || 'unknown'}
                  </div>
                  <div className="col-span-3">
                    <StatusBadge status={attestationState as any} />
                  </div>
                  <div className="col-span-3 flex justify-end gap-2">
                    <Button 
                      variant="outline" 
                      size="sm"
                      disabled={!isRunning || attestMutation.isPending}
                      onClick={() => attestMutation.mutate(vm.id)}
                    >
                      Trigger Check
                    </Button>
                    <Button 
                      variant="secondary" 
                      size="sm"
                      onClick={() => setSelectedResult({ vmId: vm.id, pcr: '0x123...abc' })}
                    >
                      View PCRs
                    </Button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </Card>

      <Dialog open={!!selectedResult} onOpenChange={(open) => !open && setSelectedResult(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Attestation Results: {selectedResult?.vmId?.slice(0, 12)}</DialogTitle>
          </DialogHeader>
          <div className="p-4 bg-muted rounded-md font-mono text-xs text-foreground overflow-auto">
            {JSON.stringify(selectedResult, null, 2)}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
