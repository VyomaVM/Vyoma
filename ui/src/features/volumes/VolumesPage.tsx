import { Database } from 'lucide-react';
import { useVolumes } from '../../hooks/queries/useVolumes';
import { Card, EmptyState, Skeleton } from '../../components/ui';

export function VolumesPage() {
  const { data, isLoading } = useVolumes();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-foreground mb-6">Volumes</h2>
      <Card>
        <div className="p-4 border-b border-border text-xs font-semibold text-muted-foreground uppercase bg-card">Volume Name / Path</div>
        <div className="divide-y divide-border/50 bg-card">
          {isLoading ? (
            Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="p-4 space-y-2">
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-3 w-48" />
              </div>
            ))
          ) : !data?.length ? (
            <EmptyState title="No volumes" description="Create a volume to get started." icon={<Database size={48} />} />
          ) : (
            data.map((v, i) => (
              <div key={i} className="p-4 text-sm text-foreground">
                <div className="font-medium">{v.name}</div>
                <div className="text-xs text-muted-foreground font-mono mt-1">{v.path}</div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
