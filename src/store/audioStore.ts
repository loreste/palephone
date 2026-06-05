import { create } from "zustand";
import type { AudioDevice } from "@/types";

interface AudioState {
  inputDevices: AudioDevice[];
  outputDevices: AudioDevice[];
  selectedInputId: string | null;
  selectedOutputId: string | null;
  inputLevel: number;
  outputLevel: number;
  volume: number;

  setInputDevices: (devices: AudioDevice[]) => void;
  setOutputDevices: (devices: AudioDevice[]) => void;
  setSelectedInputId: (id: string | null) => void;
  setSelectedOutputId: (id: string | null) => void;
  setInputLevel: (level: number) => void;
  setOutputLevel: (level: number) => void;
  setVolume: (volume: number) => void;
}

export const useAudioStore = create<AudioState>((set) => ({
  inputDevices: [],
  outputDevices: [],
  selectedInputId: null,
  selectedOutputId: null,
  inputLevel: 0,
  outputLevel: 0,
  volume: 0.8,

  setInputDevices: (devices) => set({ inputDevices: devices }),
  setOutputDevices: (devices) => set({ outputDevices: devices }),
  setSelectedInputId: (id) => set({ selectedInputId: id }),
  setSelectedOutputId: (id) => set({ selectedOutputId: id }),
  setInputLevel: (level) => set({ inputLevel: level }),
  setOutputLevel: (level) => set({ outputLevel: level }),
  setVolume: (volume) => set({ volume }),
}));
