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

  const resizeDisk = useMutation({
    mutationFn: ({ id, disk_id, desired_size }: { id: string; disk_id?: string; desired_size: number }) =>
      api.post<{ success: boolean; message: string }>(`/vms/${id}/resize-disk`, {
        disk_id: disk_id || 'rootfs',
        desired_size,
      }),
    onSuccess: handleSuccess,
  });

  const teleportVm = useMutation({
    mutationFn: ({ id, target_node_ip, bandwidth_mbps }: { id: string; target_node_ip: string; bandwidth_mbps?: number }) =>
      api.post<{ session_id: string; status: string; message: string }>('/teleport', {
        vm_id: id,
        target_node_ip,
        bandwidth_mbps,
      }),
    onSuccess: handleSuccess,
  });

  const cancelTeleport = useMutation({
    mutationFn: (id: string) =>
      api.post<{ success: boolean; message: string }>(`/teleport/cancel/${id}`),
    onSuccess: handleSuccess,
  });

  return {
    startVm,
    stopVm,
    pauseVm,
    resumeVm,
    deleteVm,
    resizeDisk,
    teleportVm,
    cancelTeleport,
  };
}
