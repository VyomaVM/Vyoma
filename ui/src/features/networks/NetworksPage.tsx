import { Globe, Plus } from 'lucide-react';
import { useNetworks } from '../../hooks/queries/useNetworks';
import { Card, EmptyState, Loading, Button } from '../../components/ui';

export function NetworksPage() {
  const { data, isLoading } = useNetworks();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <header className="flex items-center justify-between mb-6">
        <h2 className="text-2xl font-bold text-foreground">Networks</h2>
        <Button>
          <Plus size={16} /> Create Network
        </Button>
      </header>
      
      <Card>
        <div className="grid grid-cols-2 gap-4 p-4 border-b border-border text-xs font-semibold text-muted-foreground uppercase bg-card">
          <div>Network Name</div>
          <div>Subnet</div>
        </div>
        <div className="divide-y divide-border/50 bg-card">
          {isLoading ? (
            <Loading text="Loading networks..." />
          ) : !data?.networks?.length ? (
            <EmptyState title="No networks" description="Create a network to get started." icon={<Globe size={48} />} />
          ) : (
            data.networks.map((n, i) => (
              <div key={i} className="grid grid-cols-2 gap-4 p-4 text-sm text-foreground">
                <div className="flex items-center gap-2">
                  <Globe size={14} className="text-primary" /> {n.name}
                </div>
                <div className="font-mono text-muted-foreground">{n.subnet}</div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
