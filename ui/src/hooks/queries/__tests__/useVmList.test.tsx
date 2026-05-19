import type { ReactNode } from 'react';
import { renderHook, waitFor } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useVmList } from '../useVmList';
import { api } from '../../../lib/api-client';

vi.mock('../../../lib/api-client', () => ({
  api: {
    get: vi.fn(),
  },
}));

describe('useVmList', () => {
  it('should fetch and return a list of vms', async () => {
    const mockVms = {
      vms: [
        { id: '1', labels: { 'vyoma.service': 'test-vm' }, status: 'Running' }
      ]
    };
    
    (api.get as any).mockResolvedValueOnce(mockVms);

    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    );

    const { result } = renderHook(() => useVmList(), { wrapper });

    expect(result.current.isLoading).toBe(true);

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(mockVms);
    expect(api.get).toHaveBeenCalledWith('/ps');
  });
});
