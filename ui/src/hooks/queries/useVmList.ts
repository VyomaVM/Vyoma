import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export interface Vm {
  id: string;
  name: string;
  status: 'running' | 'stopped' | 'error' | 'paused' | 'pending_attestation' | 'attestation_failed';
  cpus: number;
  memory: number;
  uptime?: string;
  image: string;
}

export function useVmList() {
  return useQuery({
    queryKey: ['vms'],
    queryFn: async () => {
      // For now, this calls the API but we might not have the actual endpoint ready.
      // The API client will handle the request and throw if it fails.
      return api.get<Vm[]>('/ps');
    },
  });
}
