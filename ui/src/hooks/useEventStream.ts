import { useState, useEffect, useCallback } from 'react';

const API_BASE = import.meta.env.DEV ? 'http://localhost:3000' : '';

export interface EventMessage {
  id: string;
  type?: string;
  data: any;
  timestamp: number;
}

export function useEventStream(path: string | null) {
  const [events, setEvents] = useState<EventMessage[]>([]);
  const [isConnected, setIsConnected] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    if (!path) return;

    // Use withCredentials to ensure vyoma_token cookie is sent
    const eventSource = new EventSource(`${API_BASE}${path}`, {
      withCredentials: true,
    });

    eventSource.onopen = () => {
      setIsConnected(true);
      setError(null);
    };

    eventSource.onmessage = (event) => {
      try {
        const parsed = JSON.parse(event.data);
        setEvents((prev) => [...prev, {
          id: event.lastEventId || Date.now().toString() + Math.random(),
          type: event.type,
          data: parsed,
          timestamp: Date.now(),
        }]);
      } catch {
        setEvents((prev) => [...prev, {
          id: event.lastEventId || Date.now().toString() + Math.random(),
          type: event.type,
          data: event.data,
          timestamp: Date.now(),
        }]);
      }
    };

    eventSource.onerror = (err) => {
      console.error('EventSource error:', err);
      setIsConnected(false);
      setError(new Error('Connection lost'));
      // EventSource automatically attempts to reconnect
    };

    return () => {
      eventSource.close();
      setIsConnected(false);
    };
  }, [path]);

  const clearEvents = useCallback(() => {
    setEvents([]);
  }, []);

  return { events, isConnected, error, clearEvents };
}
