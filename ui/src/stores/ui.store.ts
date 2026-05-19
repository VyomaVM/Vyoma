import { create } from 'zustand';

interface UIState {
  sidebarOpen: boolean;
  selectedVmId: string | null;
  activeTabName: string | null;
  
  toggleSidebar: () => void;
  setSidebarOpen: (open: boolean) => void;
  setSelectedVmId: (id: string | null) => void;
  setActiveTabName: (name: string | null) => void;
}

export const useUIStore = create<UIState>((set) => ({
  sidebarOpen: true,
  selectedVmId: null,
  activeTabName: null,
  
  toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
  setSelectedVmId: (id) => set({ selectedVmId: id }),
  setActiveTabName: (name) => set({ activeTabName: name }),
}));
