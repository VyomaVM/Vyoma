import { Server } from 'lucide-react';
import { useSwarmNodes } from '../../hooks/queries/useSwarmNodes';
import { Card, EmptyState, Loading } from '../../components/ui';

export function ClusterPage() {
  const { data, isLoading } = useSwarmNodes();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-foreground mb-6">Cluster Stats</h2>
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
        {isLoading ? (
          <Loading text="Loading cluster stats..." />
        ) : !data?.length ? (
          <EmptyState title="No cluster nodes" description="Join a swarm to see cluster stats." icon={<Server size={48} />} />
        ) : (
          data.map((n, i) => (
            <Card key={i} className="hover:border-primary/50 transition-colors duration-200">
              <div className="flex items-center gap-3 mb-2 p-4 pb-0">
                <Server className="text-primary" />
                <div>
                  <h3 className="font-bold text-foreground">{n.hostname}</h3>
                  <div className="text-xs text-muted-foreground">{n.role}</div>
                </div>
              </div>
              <div className="space-y-2 p-4">
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">IP</span>
                  <span className="font-mono text-foreground">{n.ip}</span>
                </div>
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">CPU</span>
                  <span className="text-foreground">{(n.resources?.cpu_usage || 0).toFixed(1)}%</span>
                </div>
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">Mem</span>
                  <span className="text-foreground">{(n.resources?.memory_usage_mb || 0)} MB</span>
                </div>
              </div>
            </Card>
          ))
        )}
      </div>
    </div>
  );
}
