import { Settings } from 'lucide-react';
import { EmptyState } from '../../components/ui';

export function SettingsPage() {
  return (
    <div className="p-8 max-w-6xl mx-auto flex items-center justify-center min-h-[calc(100vh-4rem)]">
      <EmptyState 
        title="Settings" 
        description="Settings configuration coming soon." 
        icon={<Settings size={48} />} 
      />
    </div>
  );
}
