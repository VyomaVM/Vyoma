import { useAuthStore } from '../stores/auth.store';

const BASE_URL = import.meta.env.DEV ? 'http://localhost:8080' : '';

export async function apiFetch<T = unknown>(path: string, options?: RequestInit): Promise<T> {
  const token = useAuthStore.getState().token;
  
  const headers = new Headers(options?.headers);
  
  if (!headers.has('Content-Type') && options?.method !== 'GET' && options?.method !== 'DELETE') {
    headers.set('Content-Type', 'application/json');
  }
  
  if (token && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  
  const response = await fetch(`${BASE_URL}${path}`, {
    ...options,
    headers,
  });
  
  if (!response.ok) {
    let errorMessage = 'An error occurred while fetching data';
    try {
      const errorData = await response.json();
      errorMessage = errorData.message || errorData.error || errorMessage;
    } catch {
      errorMessage = await response.text() || errorMessage;
    }
    throw new Error(errorMessage);
  }
  
  // Handle empty responses
  if (response.status === 204 || response.headers.get('content-length') === '0') {
    return null as T;
  }
  
  // Try to parse JSON, fallback to text if not JSON
  const contentType = response.headers.get('content-type');
  if (contentType && contentType.includes('application/json')) {
    return response.json();
  }
  
  return response.text() as unknown as T;
}

export const api = {
  get: <T>(path: string, options?: RequestInit) => apiFetch<T>(path, { ...options, method: 'GET' }),
  post: <T>(path: string, data?: unknown, options?: RequestInit) => apiFetch<T>(path, { 
    ...options, 
    method: 'POST', 
    body: data ? JSON.stringify(data) : undefined 
  }),
  put: <T>(path: string, data?: unknown, options?: RequestInit) => apiFetch<T>(path, { 
    ...options, 
    method: 'PUT', 
    body: data ? JSON.stringify(data) : undefined 
  }),
  patch: <T>(path: string, data?: unknown, options?: RequestInit) => apiFetch<T>(path, { 
    ...options, 
    method: 'PATCH', 
    body: data ? JSON.stringify(data) : undefined 
  }),
  delete: <T>(path: string, options?: RequestInit) => apiFetch<T>(path, { ...options, method: 'DELETE' }),
};
