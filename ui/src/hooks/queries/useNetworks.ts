import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export interface Network {
  name: string;
  subnet: string;
}

export function useNetworks() {
  return useQuery({
    queryKey: ['networks'],
    queryFn: () => api.get<{ networks: Network[] }>('/networks'),
  });
}
