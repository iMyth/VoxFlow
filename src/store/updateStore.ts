import { create } from 'zustand';
import i18n from 'i18next';
import * as ipc from '../lib/ipc';
import { useToastStore } from './toastStore';

interface UpdateStore {
    updateAvailable: boolean;
    latestVersion: string;
    updateBody: string | null;
    checking: boolean;
    downloading: boolean;
    checkForUpdates: () => Promise<void>;
    installUpdate: () => Promise<void>;
}

export const useUpdateStore = create<UpdateStore>((set) => ({
    updateAvailable: false,
    latestVersion: '',
    updateBody: null,
    checking: false,
    downloading: false,

    checkForUpdates: async () => {
        set({ checking: true });
        try {
            const info = await ipc.checkForUpdates();
            set({
                updateAvailable: info.available,
                latestVersion: info.version,
                updateBody: info.body,
                checking: false,
            });
        } catch {
            set({ checking: false });
        }
    },

    installUpdate: async () => {
        set({ downloading: true });
        try {
            await ipc.installUpdate();
            useToastStore.getState().addToast(i18n.t('update.installSuccess'), 'success');
        } catch {
            useToastStore.getState().addToast(i18n.t('update.installFailed'), 'error');
            set({ downloading: false });
        }
    },
}));
