import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export interface SwarmNode {
  hostname: string;
  role: string;
  ip: string;
  resources?: {
    cpu_usage: number;
    memory_usage_mb: number;
  };
}

export function useSwarmNodes() {
  return useQuery({
    queryKey: ['swarm', 'nodes'],
    queryFn: () => api.get<SwarmNode[]>('/swarm/nodes'),
  });
}
