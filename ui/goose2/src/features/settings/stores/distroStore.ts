import { create } from "zustand";
import type { DistroBundleInfo } from "@/shared/types/distro";

interface DistroState {
  loaded: boolean;
  manifest: DistroBundleInfo;
  setManifest: (manifest: DistroBundleInfo) => void;
}

const EMPTY_DISTRO: DistroBundleInfo = {
  present: false,
};

export const useDistroStore = create<DistroState>((set) => ({
  loaded: false,
  manifest: EMPTY_DISTRO,
  setManifest: (manifest) => set({ manifest, loaded: true }),
}));
