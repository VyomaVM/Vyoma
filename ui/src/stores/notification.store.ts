import { create } from 'zustand';
import { toast } from 'sonner';

interface NotificationState {
  success: (message: string, description?: string) => void;
  error: (message: string, description?: string) => void;
  info: (message: string, description?: string) => void;
  warning: (message: string, description?: string) => void;
  dismiss: (id?: string | number) => void;
}

export const useNotificationStore = create<NotificationState>(() => ({
  success: (message, description) => toast.success(message, { description }),
  error: (message, description) => toast.error(message, { description }),
  info: (message, description) => toast.info(message, { description }),
  warning: (message, description) => toast.warning(message, { description }),
  dismiss: (id) => toast.dismiss(id),
}));
