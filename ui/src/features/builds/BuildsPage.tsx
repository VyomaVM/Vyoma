import { Wrench } from 'lucide-react';
import { EmptyState } from '../../components/ui';

export function BuildsPage() {
  return (
    <div className="p-8 max-w-6xl mx-auto flex items-center justify-center min-h-[calc(100vh-4rem)]">
      <EmptyState 
        title="Build History" 
        description="Build history is pending backend support. Use POST /build API to trigger builds." 
        icon={<Wrench size={48} />} 
      />
    </div>
  );
}
