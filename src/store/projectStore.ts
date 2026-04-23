import { create } from 'zustand';

import { useToastStore } from './toastStore';
import * as ipc from '../lib/ipc';

import type { Project, ProjectDetail } from '../types';

interface ProjectStats {
  id: string;
  name: string;
  created_at: string;
  line_count: number;
  audio_count: number;
  character_count: number;
}

interface ProjectStore {
  projects: Project[];
  projectStats: Record<string, ProjectStats>;
  currentProject: ProjectDetail | null;
  loading: boolean;
  error: string | null;
  fetchProjects: () => Promise<void>;
  fetchProjectStats: () => Promise<void>;
  createProject: (name: string) => Promise<void>;
  loadProject: (id: string) => Promise<void>;
  deleteProject: (id: string) => Promise<void>;
  saveOutline: (outline: string) => Promise<void>;
}

export const useProjectStore = create<ProjectStore>((set) => ({
  projects: [],
  projectStats: {},
  currentProject: null,
  loading: false,
  error: null,

  fetchProjects: async () => {
    set({ loading: true, error: null });
    try {
      const projects = await ipc.listProjects();
      set({ projects, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  fetchProjectStats: async () => {
    try {
      const statsList = await ipc.listProjectsWithStats();
      const statsMap: Record<string, ProjectStats> = {};
      for (const s of statsList) {
        statsMap[s.id] = s;
      }
      set({ projectStats: statsMap });
    } catch {
      // Stats are non-critical, silently ignore errors
    }
  },

  createProject: async (name: string) => {
    set({ loading: true, error: null });
    try {
      const project = await ipc.createProject(name);
      set((state) => ({
        projects: [project, ...state.projects],
        loading: false,
      }));
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  loadProject: async (id: string) => {
    set({ loading: true, error: null });
    try {
      const detail = await ipc.loadProject(id);
      set({ currentProject: detail, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  deleteProject: async (id: string) => {
    set({ loading: true, error: null });
    try {
      await ipc.deleteProject(id);
      set((state) => ({
        projects: state.projects.filter((p) => p.id !== id),
        currentProject: state.currentProject?.project.id === id ? null : state.currentProject,
        loading: false,
      }));
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  saveOutline: async (outline: string) => {
    const project = useProjectStore.getState().currentProject;
    if (!project) return;
    try {
      await ipc.saveOutline(project.project.id, outline);
      set((state) => ({
        currentProject: state.currentProject
          ? { ...state.currentProject, project: { ...state.currentProject.project, outline } }
          : state.currentProject,
      }));
    } catch {
      useToastStore.getState().addToast('project.saveOutlineFailed');
    }
  },
}));
