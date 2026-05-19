import { Skeleton } from './skeleton';

interface LoadingProps {
  text?: string;
  useSkeleton?: boolean;
}

export function Loading({ text = 'Loading...', useSkeleton }: LoadingProps) {
  if (useSkeleton) {
    return (
      <div className="space-y-3">
        <Skeleton className="h-4 w-[250px]" />
        <Skeleton className="h-4 w-[200px]" />
        <Skeleton className="h-4 w-[200px]" />
      </div>
    );
  }

  return (
    <div className="flex items-center justify-center py-12">
      <div className="text-muted-foreground animate-pulse">{text}</div>
    </div>
  );
}
