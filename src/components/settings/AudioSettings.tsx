import { useState, useEffect } from "react";
import { Headphones, Play } from "lucide-react";
import { Reorder } from "framer-motion";
import { cn } from "@/lib/cn";
import { Toggle } from "@/components/ui/Toggle";
import { useAudioStore } from "@/store/audioStore";
import { listAudioDevices, detectHidDevices, onHidHookSwitch, onHidMuteToggle, type HidAudioDevice } from "@/lib/tauri";

interface Codec {
  id: string;
  name: string;
  rate: string;
}

const defaultCodecs: Codec[] = [
  { id: "opus", name: "Opus", rate: "48 kHz" },
  { id: "g722", name: "G.722", rate: "16 kHz" },
  { id: "pcmu", name: "PCMU / G.711\u00B5", rate: "8 kHz" },
  { id: "pcma", name: "PCMA / G.711A", rate: "8 kHz" },
];

export function AudioSettings() {
  const { selectedInputId, selectedOutputId, setSelectedInputId, setSelectedOutputId } =
    useAudioStore();

  const [inputDevices, setInputDevices] = useState<{ id: string; name: string }[]>([]);
  const [outputDevices, setOutputDevices] = useState<{ id: string; name: string }[]>([]);
  const [echoCancel, setEchoCancel] = useState(true);
  const [noiseSuppression, setNoiseSuppression] = useState(true);
  const [autoGain, setAutoGain] = useState(false);

  // Load real audio devices from PJSIP on mount
  useEffect(() => {
    const { selectedInputId: curIn, selectedOutputId: curOut, setSelectedInputId: setIn, setSelectedOutputId: setOut } =
      useAudioStore.getState();

    listAudioDevices()
      .then((devices) => {
        const inputs = devices
          .filter((d) => d.input_count > 0)
          .map((d) => ({ id: String(d.id), name: d.name }));
        const outputs = devices
          .filter((d) => d.output_count > 0)
          .map((d) => ({ id: String(d.id), name: d.name }));
        setInputDevices(inputs);
        setOutputDevices(outputs);
        if (!curIn && inputs.length > 0) setIn(inputs[0].id);
        if (!curOut && outputs.length > 0) setOut(outputs[0].id);
      })
      .catch(() => {
        setInputDevices([{ id: "0", name: "Default Microphone" }]);
        setOutputDevices([{ id: "0", name: "Default Speaker" }]);
      });
  }, []);
  const [codecs, setCodecs] = useState(defaultCodecs);

  return (
    <div className="space-y-5">
      {/* Audio Devices */}
      <SectionHeader title="Audio Devices" />

      <DeviceSelect
        label="Microphone"
        devices={inputDevices}
        value={selectedInputId ?? "default-in"}
        onChange={setSelectedInputId}
      />
      <MicLevelMeter />

      <DeviceSelect
        label="Speaker"
        devices={outputDevices}
        value={selectedOutputId ?? "default-out"}
        onChange={setSelectedOutputId}
      />
      <button
        className={cn(
          "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium",
          "bg-elevated text-secondary hover:bg-overlay transition-colors"
        )}
      >
        <Play size={12} />
        Test Speaker
      </button>

      {/* Audio Processing */}
      <SectionHeader title="Audio Processing" />

      <ToggleRow
        label="Echo Cancellation"
        checked={echoCancel}
        onChange={setEchoCancel}
      />
      <ToggleRow
        label="Noise Suppression"
        checked={noiseSuppression}
        onChange={setNoiseSuppression}
      />
      <ToggleRow
        label="Auto Gain Control"
        checked={autoGain}
        onChange={setAutoGain}
      />

      {/* Connected Headset / HID Devices */}
      <SectionHeader title="Connected Headsets" />
      <HidDevicesSection />

      {/* Codec Priority */}
      <SectionHeader title="Codec Priority" />
      <p className="text-[10px] text-tertiary">Drag to reorder preference</p>

      <Reorder.Group
        axis="y"
        values={codecs}
        onReorder={setCodecs}
        className="space-y-1"
      >
        {codecs.map((codec) => (
          <Reorder.Item
            key={codec.id}
            value={codec}
            className={cn(
              "flex items-center gap-3 px-3 py-2 rounded-lg",
              "bg-surface border border-border-subtle cursor-grab active:cursor-grabbing",
              "hover:bg-elevated transition-colors"
            )}
          >
            <span className="text-tertiary text-xs select-none">\u2261</span>
            <span className="text-sm text-primary flex-1">{codec.name}</span>
            <span className="text-[10px] text-tertiary">{codec.rate}</span>
          </Reorder.Item>
        ))}
      </Reorder.Group>
    </div>
  );
}

/** Simulated microphone level meter */
function MicLevelMeter() {
  const [level, setLevel] = useState(0);

  useEffect(() => {
    // Simulate mic input for Phase 2
    const interval = setInterval(() => {
      setLevel(Math.random() * 0.7 + Math.sin(Date.now() / 200) * 0.15);
    }, 66); // ~15fps
    return () => clearInterval(interval);
  }, []);

  const clampedLevel = Math.max(0, Math.min(1, level));
  const segments = 20;
  const activeSegments = Math.round(clampedLevel * segments);

  return (
    <div className="flex items-center gap-0.5 h-3 px-1" aria-label={`Mic level: ${Math.round(clampedLevel * 100)}%`}>
      {Array.from({ length: segments }).map((_, i) => {
        const isActive = i < activeSegments;
        const color =
          i < segments * 0.7
            ? "bg-success"
            : i < segments * 0.9
              ? "bg-warning"
              : "bg-destructive";
        return (
          <div
            key={i}
            className={cn(
              "flex-1 h-2 rounded-[1px] transition-colors",
              isActive ? color : "bg-elevated"
            )}
          />
        );
      })}
    </div>
  );
}

function DeviceSelect({
  label,
  devices,
  value,
  onChange,
}: {
  label: string;
  devices: { id: string; name: string }[];
  value: string;
  onChange: (id: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium text-secondary">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className={cn(
          "w-full bg-surface border border-border-subtle rounded-md",
          "px-3 py-2 text-sm text-primary",
          "focus:outline-none focus:border-border-focus focus:ring-1 focus:ring-accent/30"
        )}
      >
        {devices.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
          </option>
        ))}
      </select>
    </div>
  );
}

function ToggleRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="text-sm text-primary">{label}</span>
      <Toggle checked={checked} onChange={onChange} label={label} />
    </div>
  );
}

function HidDevicesSection() {
  const [hidDevices, setHidDevices] = useState<HidAudioDevice[]>([]);

  useEffect(() => {
    detectHidDevices()
      .then(setHidDevices)
      .catch(() => setHidDevices([]));

    // Listen for HID events
    const unsubHook = onHidHookSwitch(({ action }) => {
      // Hook switch events (answer/hangup) are handled by the call components
      console.log("[HID] hook_switch:", action);
    });
    const unsubMute = onHidMuteToggle(({ muted }) => {
      console.log("[HID] mute_toggle:", muted);
    });

    return () => {
      unsubHook.then((fn) => fn());
      unsubMute.then((fn) => fn());
    };
  }, []);

  const headsets = hidDevices.filter((d) => d.device_type === "headset");

  if (headsets.length === 0) {
    return <p className="text-xs text-tertiary">No headsets detected. Connect a USB/Bluetooth headset for call control.</p>;
  }

  return (
    <div className="space-y-1">
      {headsets.map((d, i) => (
        <div key={i} className="flex items-center gap-2 py-1.5 px-2 rounded bg-elevated">
          <Headphones size={14} className="text-accent" />
          <span className="text-sm text-primary flex-1">{d.name}</span>
          <span className="text-[10px] text-green-600">Connected</span>
        </div>
      ))}
      <p className="text-[10px] text-tertiary">Hook switch and mute button events are active.</p>
    </div>
  );
}

function SectionHeader({ title }: { title: string }) {
  return (
    <h3 className="text-xs font-semibold text-tertiary uppercase tracking-wider pt-2">
      {title}
    </h3>
  );
}
