import create from "zustand";
import { SimSnapshot } from "./api";

type State = {
  snapshot?: SimSnapshot;
  history: SimSnapshot[];
  loading: boolean;
  setSnapshot: (s: SimSnapshot) => void;
  setLoading: (b: boolean) => void;
};

export const useAppStore = create<State>((set) => ({
  snapshot: undefined,
  history: [],
  loading: false,
  setSnapshot: (s) => set((st) => ({ snapshot: s, history: [...st.history, s] })),
  setLoading: (b) => set({ loading: b }),
}));

