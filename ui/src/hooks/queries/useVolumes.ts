import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export interface Volume {
  name: string;
  path: string;
}

export function useVolumes() {
  return useQuery({
    queryKey: ['volumes'],
    queryFn: () => api.get<Volume[]>('/volumes'),
  });
}
