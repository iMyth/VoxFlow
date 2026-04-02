import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import type { Project, ProjectDetail } from '../types';

interface ProjectStore {
    projects: Project[];
    currentProject: ProjectDetail | null;
    loading: boolean;
    error: string | null;
    fetchProjects: () => Promise<void>;
    createProject: (name: string) => Promise<void>;
    loadProject: (id: string) => Promise<void>;
    deleteProject: (id: string) => Promise<void>;
}

export const useProjectStore = create<ProjectStore>((set) => ({
    projects: [],
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

    createProject: async (name: string) => {
        set({ loading: true, error: null });
        try {
            const project = await ipc.createProject(name);
            set((state) => ({
                projects: [...state.projects, project],
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
}));
