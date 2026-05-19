import { useVmList } from '../../hooks/queries/useVmList';
import { useImages } from '../../hooks/queries/useImages';
import { useEventStream } from '../../hooks/useEventStream';
import { Card } from '../../components/ui';
import { Box, HardDrive, Zap, Activity } from 'lucide-react';

export function DashboardPage() {
  const { data: vmsData, isLoading: vmsLoading } = useVmList();
  const { data: imagesData, isLoading: imagesLoading } = useImages();
  const { events, isConnected } = useEventStream('/events');

  const runningVms = vmsData?.vms?.filter(v => v.status?.startsWith('Running'))?.length || 0;
  const totalImages = imagesData?.length || 0;
  
  // Show last 5 events
  const recentEvents = [...events].reverse().slice(0, 5);

  return (
    <div className="p-8 max-w-6xl mx-auto space-y-8">
      <header>
        <h2 className="text-2xl font-bold text-foreground mb-1">Dashboard</h2>
        <p className="text-sm text-muted-foreground">Overview of your Vyoma environment.</p>
      </header>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <Card className="p-6 flex flex-col justify-between h-32 hover:border-primary/50 transition">
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground font-medium">Running VMs</span>
            <Box size={20} className="text-primary" />
          </div>
          <div className="text-3xl font-bold text-foreground">
            {vmsLoading ? <span className="animate-pulse">--</span> : runningVms}
            <span className="text-sm text-muted-foreground font-normal ml-2">/ {vmsData?.vms?.length || 0} total</span>
          </div>
        </Card>

        <Card className="p-6 flex flex-col justify-between h-32 hover:border-primary/50 transition">
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground font-medium">Local Images</span>
            <HardDrive size={20} className="text-primary" />
          </div>
          <div className="text-3xl font-bold text-foreground">
            {imagesLoading ? <span className="animate-pulse">--</span> : totalImages}
          </div>
        </Card>

        <Card className="p-6 flex flex-col justify-between h-32 hover:border-primary/50 transition">
          <div className="flex items-center justify-between">
            <span className="text-muted-foreground font-medium">Event Stream</span>
            <Activity size={20} className={isConnected ? "text-green-500" : "text-muted-foreground"} />
          </div>
          <div className="text-3xl font-bold text-foreground">
            {events.length}
            <span className="text-sm text-muted-foreground font-normal ml-2">events seen</span>
          </div>
        </Card>
      </div>

      <Card>
        <div className="p-4 border-b border-border flex items-center justify-between bg-card">
          <div className="font-semibold text-foreground flex items-center gap-2">
            <Zap size={16} className="text-primary" /> Recent Events
          </div>
          {isConnected && (
            <div className="flex items-center gap-2 text-xs text-green-500">
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-green-400 opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-green-500"></span>
              </span>
              Live
            </div>
          )}
        </div>
        <div className="divide-y divide-border/50 bg-card">
          {events.length === 0 ? (
            <div className="p-8 text-center text-muted-foreground text-sm">
              Waiting for events to arrive...
            </div>
          ) : (
            recentEvents.map((ev) => (
              <div key={ev.id} className="p-4 flex flex-col gap-1 text-sm">
                <div className="flex justify-between items-start">
                  <span className="font-mono text-primary font-medium">{ev.type || 'message'}</span>
                  <span className="text-xs text-muted-foreground">
                    {new Date(ev.timestamp).toLocaleTimeString()}
                  </span>
                </div>
                <div className="text-muted-foreground font-mono text-xs mt-1 bg-muted/50 p-2 rounded">
                  {typeof ev.data === 'string' ? ev.data : JSON.stringify(ev.data)}
                </div>
              </div>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
