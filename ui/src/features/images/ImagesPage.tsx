import { HardDrive } from 'lucide-react';
import { useImages } from '../../hooks/queries/useImages';
import { Card, EmptyState, Skeleton } from '../../components/ui';

export function ImagesPage() {
  const { data, isLoading } = useImages();

  return (
    <div className="p-8 max-w-6xl mx-auto">
      <h2 className="text-2xl font-bold text-foreground mb-6">Images</h2>
      <Card>
        <div className="grid grid-cols-3 gap-4 p-4 border-b border-border text-xs font-semibold text-muted-foreground uppercase bg-card">
          <div>Repository</div>
          <div>Tag</div>
          <div className="text-right">Size</div>
        </div>
        <div className="divide-y divide-border/50 bg-card">
          {isLoading ? (
            Array.from({ length: 3 }).map((_, i) => (
              <div key={i} className="grid grid-cols-3 gap-4 p-4 items-center">
                <div><Skeleton className="h-4 w-32" /></div>
                <div><Skeleton className="h-4 w-16" /></div>
                <div className="flex justify-end"><Skeleton className="h-4 w-12" /></div>
              </div>
            ))
          ) : !data?.length ? (
            <EmptyState title="No images" description="Pull an image to get started." icon={<HardDrive size={48} />} />
          ) : (
            data.map((img, i) => (
              <div key={i} className="grid grid-cols-3 gap-4 p-4 text-sm text-foreground">
                <div>{img}</div>
                <div>latest</div>
                <div className="text-right text-muted-foreground font-mono">--</div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
