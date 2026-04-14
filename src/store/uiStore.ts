import { create } from 'zustand';

interface UiState {
  pendingExtraInstructions: string | null;
  pendingCharNames: string[] | null;
  setPendingExtraInstructions: (instructions: string | null) => void;
  setPendingCharNames: (names: string[] | null) => void;
  clearPending: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  pendingExtraInstructions: null,
  pendingCharNames: null,
  setPendingExtraInstructions: (instructions) => {
    set({ pendingExtraInstructions: instructions });
  },
  setPendingCharNames: (names) => {
    set({ pendingCharNames: names });
  },
  clearPending: () => {
    set({ pendingExtraInstructions: null, pendingCharNames: null });
  },
}));
