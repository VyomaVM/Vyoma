import { useQuery } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export function useImages() {
  return useQuery({
    queryKey: ['images'],
    queryFn: () => api.get<string[]>('/images'),
  });
}
