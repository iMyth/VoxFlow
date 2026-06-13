import { create } from 'zustand';

import * as ipc from '../lib/ipc';

import type { ResourceEntry, ResourceSummary } from '../lib/ipc';

type ResourceFilter = 'all' | 'audio' | 'video' | 'composition' | 'bgm' | 'export';

interface ResourceState {
  resources: ResourceEntry[];
  summary: ResourceSummary | null;
  loading: boolean;
  filter: ResourceFilter;
  selectedIds: Set<string>;
  error: string | null;

  // Actions
  fetchResources: (projectId: string) => Promise<void>;
  fetchSummary: (projectId: string) => Promise<void>;
  setFilter: (filter: ResourceFilter) => void;
  toggleSelection: (id: string) => void;
  selectAll: () => void;
  clearSelection: () => void;
  deleteSelected: (projectId: string) => Promise<void>;
  deleteSingle: (projectId: string, filePath: string) => Promise<void>;
  openFolder: (projectId: string, subfolder?: string) => Promise<void>;
}

export const useResourceStore = create<ResourceState>((set, get) => ({
  resources: [],
  summary: null,
  loading: false,
  filter: 'all',
  selectedIds: new Set(),
  error: null,

  fetchResources: async (projectId: string) => {
    set({ loading: true, error: null });
    try {
      const resources = await ipc.listResources(projectId);
      set({ resources, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  fetchSummary: async (projectId: string) => {
    try {
      const summary = await ipc.getResourceSummary(projectId);
      set({ summary });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setFilter: (filter: ResourceFilter) => {
    set({ filter, selectedIds: new Set() });
  },

  toggleSelection: (id: string) => {
    const { selectedIds } = get();
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    set({ selectedIds: next });
  },

  selectAll: () => {
    const { resources, filter } = get();
    const filtered = filter === 'all' ? resources : resources.filter((r) => r.resource_type === filter);
    set({ selectedIds: new Set(filtered.map((r) => r.file_path)) });
  },

  clearSelection: () => {
    set({ selectedIds: new Set() });
  },

  deleteSelected: async (projectId: string) => {
    const { selectedIds } = get();
    if (selectedIds.size === 0) return;

    try {
      await ipc.deleteResourcesBatch(projectId, Array.from(selectedIds));
      set({ selectedIds: new Set() });
      // Refresh
      await get().fetchResources(projectId);
      await get().fetchSummary(projectId);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  deleteSingle: async (projectId: string, filePath: string) => {
    try {
      await ipc.deleteResource(projectId, filePath);
      // Refresh
      await get().fetchResources(projectId);
      await get().fetchSummary(projectId);
    } catch (e) {
      set({ error: String(e) });
    }
  },

  openFolder: async (projectId: string, subfolder?: string) => {
    try {
      await ipc.openResourceFolder(projectId, subfolder);
    } catch (e) {
      set({ error: String(e) });
    }
  },
}));
