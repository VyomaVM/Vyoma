import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '../../lib/api-client';

export interface Vm {
  id: string;
  labels: Record<string, string>;
  status?: string;
  ip_address?: string;
}

export function useVmList() {
  return useQuery({
    queryKey: ['vms'],
    queryFn: () => api.get<{ vms: Vm[] }>('/ps'),
  });
}

export function useVmMutations() {
  const queryClient = useQueryClient();

  const handleSuccess = () => {
    queryClient.invalidateQueries({ queryKey: ['vms'] });
  };

  const startVm = useMutation({
    mutationFn: (id: string) => api.post(`/vms/${id}/start`),
    onSuccess: handleSuccess,
  });

  const stopVm = useMutation({
    mutationFn: (id: string) => api.post(`/vms/${id}/stop`),
    onSuccess: handleSuccess,
  });

  const pauseVm = useMutation({
    mutationFn: (id: string) => api.post(`/vms/${id}/pause`),
    onSuccess: handleSuccess,
  });

  const resumeVm = useMutation({
    mutationFn: (id: string) => api.post(`/vms/${id}/resume`),
    onSuccess: handleSuccess,
  });

  const deleteVm = useMutation({
    mutationFn: (id: string) => api.delete(`/vms/${id}`),
    onSuccess: handleSuccess,
  });

  return {
    startVm,
    stopVm,
    pauseVm,
    resumeVm,
    deleteVm,
  };
}
