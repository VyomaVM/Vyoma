import { Badge } from './badge';
import { clsx } from 'clsx';

interface StatusBadgeProps {
  status: 'running' | 'stopped' | 'error' | 'paused' | 'pending_attestation' | 'attestation_failed';
}

export function StatusBadge({ status }: StatusBadgeProps) {
  const colors = {
    running: 'bg-green-500/10 text-green-500 hover:bg-green-500/20 shadow-[0_0_8px_rgba(34,197,94,0.4)]',
    stopped: 'bg-slate-500/10 text-slate-500 hover:bg-slate-500/20',
    error: 'bg-destructive/10 text-destructive hover:bg-destructive/20',
    paused: 'bg-yellow-500/10 text-yellow-500 hover:bg-yellow-500/20',
    pending_attestation: 'bg-yellow-400/10 text-yellow-400 hover:bg-yellow-400/20 animate-pulse',
    attestation_failed: 'bg-destructive/10 text-destructive hover:bg-destructive/20 shadow-[0_0_8px_rgba(239,68,68,0.4)]',
  };

  return (
    <Badge variant="outline" className={clsx('font-mono uppercase text-xs border-transparent', colors[status])}>
      {status.replace('_', ' ')}
    </Badge>
  );
}
