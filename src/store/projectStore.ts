import { create } from 'zustand';
import * as ipc from '../lib/ipc';
import { useToastStore } from './toastStore';
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
    saveOutline: (outline: string) => Promise<void>;
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
        } catch (e) {
            useToastStore.getState().addToast('project.saveOutlineFailed');
        }
    },
}));
