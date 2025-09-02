import create from "zustand";
import { SimSnapshot, SimStateDto, SimListsDto } from "./api";

type State = {
  snapshot?: SimSnapshot;
  history: SimSnapshot[];
  loading: boolean;
  isBusy: boolean;
  error?: string;
  toast?: string;
  stateDto?: SimStateDto;
  lists?: SimListsDto;
  setSnapshot: (s: SimSnapshot) => void;
  setLoading: (b: boolean) => void;
  setBusy: (b: boolean) => void;
  setError: (e?: string) => void;
  showToast: (msg: string) => void;
  setStateDto: (s: SimStateDto) => void;
  setLists: (l: SimListsDto) => void;
};

export const useAppStore = create<State>((set) => ({
  snapshot: undefined,
  history: [],
  loading: false,
  isBusy: false,
  setSnapshot: (s) => set((st) => ({ snapshot: s, history: [...st.history, s] })),
  setLoading: (b) => set({ loading: b }),
  setBusy: (b) => set({ isBusy: b }),
  setError: (e) => set({ error: e }),
  showToast: (msg) => { set({ toast: msg }); setTimeout(() => set({ toast: undefined }), 2000); },
  setStateDto: (s) => set({ stateDto: s }),
  setLists: (l) => set({ lists: l }),
}));
