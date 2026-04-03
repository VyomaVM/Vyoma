import { useState, useEffect } from 'react';

const API_BASE = 'http://localhost:3000';

export function useApi<T>(endpoint: string, options?: { autoFetch?: boolean }) {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const doFetch = async () => {
    if (!API_BASE) return;
    setLoading(true);
    setError(null);
    try {
      const res = await globalThis.fetch(`${API_BASE}${endpoint}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const json = await res.json();
      setData(json);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (options?.autoFetch !== false) doFetch();
  }, [endpoint]);

  return { data, loading, error, refetch: doFetch };
}

export function useVmList() {
  return useApi<{ vms: Vm[] }>('/ps');
}

export function useImages() {
  return useApi<string[]>('/images');
}

export function useVolumes() {
  return useApi<Volume[]>('/volumes');
}

export function useNetworks() {
  return useApi<{ networks: Network[] }>('/networks');
}

export function useSwarmNodes() {
  return useApi<SwarmNode[]>('/swarm/nodes');
}

export function useSnapshots(vmId: string) {
  return useApi<{ snapshots: Snapshot[] }>(vmId ? `/snapshots/${vmId}` : '');
}

export interface Vm {
  id: string;
  labels: Record<string, string>;
  status?: string;
  ip_address?: string;
}

export interface Volume {
  name: string;
  path: string;
}

export interface Network {
  name: string;
  subnet: string;
}

export interface SwarmNode {
  hostname: string;
  role: string;
  ip: string;
  resources?: {
    cpu_usage: number;
    memory_usage_mb: number;
  };
}

export interface Snapshot {
  id: string;
  label?: string;
  created_at: number;
  size_bytes: number;
}
