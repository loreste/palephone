/**
 * LiveKitRoom -- connects to a LiveKit SFU room and renders remote/local
 * audio & video tracks.  Provides screen-sharing controls.
 *
 * This component is rendered by ActiveCallView when the join response
 * includes `livekit_url` and `livekit_token`.
 *
 * The `livekit-client` package is loaded dynamically at runtime so the rest
 * of the app compiles even before `npm install` adds the package.  All
 * LiveKit types are typed as `any` to avoid compile-time dependency on the
 * module declaration.
 */
import { useState, useEffect, useRef, useCallback } from "react";
import {
  Mic,
  MicOff,
  Video,
  VideoOff,
  MonitorUp,
  MonitorOff,
  PhoneOff,
} from "lucide-react";
import { cn } from "@/lib/cn";
import { useMeetingStore } from "@/store/meetingStore";
import { useServerStore } from "@/store/serverStore";
import { paleServerApi } from "@/lib/tauri";
import { toast } from "@/components/ui/Toast";

// ---------------------------------------------------------------------------
// Dynamic import of livekit-client (loaded lazily at runtime)
// ---------------------------------------------------------------------------

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let lkMod: any = null;

// Module name stored in a variable so TypeScript does not attempt to
// resolve it at compile time (the package may not be installed yet).
const LK_MODULE = "livekit-client";

async function getLiveKit() {
  if (!lkMod) {
    try {
      lkMod = await import(/* @vite-ignore */ LK_MODULE);
    } catch {
      return null;
    }
  }
  return lkMod;
}

// ---------------------------------------------------------------------------
// Track renderer -- attaches a LiveKit Track to a <video> element.
// ---------------------------------------------------------------------------

function TrackRenderer({
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  track,
  className,
  muted,
}: {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  track: any;
  className?: string;
  muted?: boolean;
}) {
  const ref = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el || !track) return;
    track.attach(el);
    return () => {
      track.detach(el);
    };
  }, [track]);

  if (!track) return null;
  return (
    <video
      ref={ref}
      autoPlay
      playsInline
      muted={muted}
      className={cn("w-full h-full object-cover rounded", className)}
    />
  );
}

// ---------------------------------------------------------------------------
// Remote track descriptor
// ---------------------------------------------------------------------------

interface RemoteTrackEntry {
  participantSid: string;
  trackSid: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  track: any;
  kind: string;
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function LiveKitRoom({
  conferenceId,
}: {
  conferenceId: string;
}) {
  const livekitUrl = useMeetingStore((s) => s.livekitUrl);
  const livekitToken = useMeetingStore((s) => s.livekitToken);
  const setLiveKitCredentials = useMeetingStore((s) => s.setLiveKitCredentials);
  const baseUrl = useServerStore((s) => s.baseUrl);
  const serverToken = useServerStore((s) => s.token);

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [room, setRoom] = useState<any>(null);
  const [connected, setConnected] = useState(false);
  const [audioEnabled, setAudioEnabled] = useState(true);
  const [videoEnabled, setVideoEnabled] = useState(true);
  const [screenSharing, setScreenSharing] = useState(false);
  const [remoteTracks, setRemoteTracks] = useState<RemoteTrackEntry[]>([]);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const [localVideoTrack, setLocalVideoTrack] = useState<any>(null);

  // Connect to LiveKit room
  useEffect(() => {
    if (!livekitUrl || !livekitToken) return;

    let cancelled = false;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let currentRoom: any = null;

    (async () => {
      const lk = await getLiveKit();
      if (!lk || cancelled) return;

      const newRoom = new lk.Room({
        adaptiveStream: true,
        dynacast: true,
      });

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const handleTrackSubscribed = (track: any, _pub: any, participant: any) => {
        setRemoteTracks((prev) => [
          ...prev,
          {
            participantSid: participant.sid,
            trackSid: track.sid,
            track,
            kind: track.kind,
          },
        ]);
      };

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const handleTrackUnsubscribed = (track: any) => {
        setRemoteTracks((prev) =>
          prev.filter((t) => t.trackSid !== track.sid),
        );
      };

      newRoom
        .on(lk.RoomEvent.TrackSubscribed, handleTrackSubscribed)
        .on(lk.RoomEvent.TrackUnsubscribed, handleTrackUnsubscribed)
        .on(lk.RoomEvent.Disconnected, () => {
          setConnected(false);
          setRemoteTracks([]);
          setLocalVideoTrack(null);
        });

      try {
        await newRoom.connect(livekitUrl, livekitToken);
        if (cancelled) {
          await newRoom.disconnect();
          return;
        }
        currentRoom = newRoom;
        setRoom(newRoom);
        setConnected(true);

        // Publish local audio & video
        await newRoom.localParticipant.enableCameraAndMicrophone();

        // Extract local camera track
        for (const pub of newRoom.localParticipant.trackPublications.values()) {
          if (pub.track && pub.source === lk.Track.Source.Camera) {
            setLocalVideoTrack(pub.track);
          }
        }
      } catch (err) {
        console.error("LiveKit connect error:", err);
        toast({ type: "error", title: "Failed to connect to media server" });
      }
    })();

    return () => {
      cancelled = true;
      if (currentRoom) {
        currentRoom.disconnect().catch(() => {});
      }
    };
  }, [livekitUrl, livekitToken]);

  // Toggle microphone
  const toggleAudio = useCallback(async () => {
    if (!room) return;
    try {
      await room.localParticipant.setMicrophoneEnabled(!audioEnabled);
      setAudioEnabled(!audioEnabled);
    } catch {
      toast({ type: "error", title: "Failed to toggle microphone" });
    }
  }, [room, audioEnabled]);

  // Toggle camera
  const toggleVideo = useCallback(async () => {
    if (!room) return;
    try {
      await room.localParticipant.setCameraEnabled(!videoEnabled);
      setVideoEnabled(!videoEnabled);
    } catch {
      toast({ type: "error", title: "Failed to toggle camera" });
    }
  }, [room, videoEnabled]);

  // Toggle screen share
  const toggleScreenShare = useCallback(async () => {
    if (!room) return;
    try {
      await room.localParticipant.setScreenShareEnabled(!screenSharing);
      setScreenSharing(!screenSharing);
    } catch {
      toast({ type: "error", title: "Failed to toggle screen sharing" });
    }
  }, [room, screenSharing]);

  // Leave the LiveKit room
  const leave = useCallback(async () => {
    if (room) {
      await room.disconnect();
    }
    setRoom(null);
    setConnected(false);
    setRemoteTracks([]);
    setLocalVideoTrack(null);
    setLiveKitCredentials(null, null);
  }, [room, setLiveKitCredentials]);

  // Reconnect with a fresh token
  const reconnect = useCallback(async () => {
    if (!baseUrl || !serverToken) return;
    try {
      const resp = await paleServerApi<{
        livekit_url: string;
        livekit_token: string;
      }>(baseUrl, serverToken, `/v1/conferences/${conferenceId}/media-token`);
      setLiveKitCredentials(resp.livekit_url, resp.livekit_token);
    } catch {
      toast({ type: "error", title: "Failed to refresh media token" });
    }
  }, [baseUrl, serverToken, conferenceId, setLiveKitCredentials]);

  if (!livekitUrl || !livekitToken) {
    return null;
  }

  const remoteVideoTracks = remoteTracks.filter((t) => t.kind === "video");

  return (
    <div className="flex flex-col h-full bg-black relative">
      {/* Connection status */}
      {!connected && (
        <div className="absolute inset-0 flex items-center justify-center bg-black/80 z-10">
          <div className="text-white text-sm">
            Connecting to media server...
            <button
              onClick={reconnect}
              className="ml-2 text-accent underline text-xs"
            >
              Retry
            </button>
          </div>
        </div>
      )}

      {/* Video grid */}
      <div className="flex-1 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-1 p-1 auto-rows-fr">
        {/* Local video (self-view) */}
        {localVideoTrack && (
          <div className="relative rounded overflow-hidden bg-gray-900">
            <TrackRenderer track={localVideoTrack} muted />
            <span className="absolute bottom-1 left-1 bg-black/60 text-white text-[10px] px-1.5 py-0.5 rounded">
              You
            </span>
          </div>
        )}

        {/* Remote video tracks */}
        {remoteVideoTracks.map((rt) => (
          <div
            key={rt.trackSid}
            className="relative rounded overflow-hidden bg-gray-900"
          >
            <TrackRenderer track={rt.track} />
            <span className="absolute bottom-1 left-1 bg-black/60 text-white text-[10px] px-1.5 py-0.5 rounded">
              {rt.participantSid.slice(0, 8)}
            </span>
          </div>
        ))}

        {/* Placeholder when no video */}
        {!localVideoTrack && remoteVideoTracks.length === 0 && (
          <div className="flex items-center justify-center text-gray-500 text-sm col-span-full">
            No video tracks
          </div>
        )}
      </div>

      {/* Controls bar */}
      <div className="flex items-center justify-center gap-3 p-3 bg-gray-900/80">
        <button
          onClick={toggleAudio}
          className={cn(
            "p-3 rounded-full",
            audioEnabled
              ? "bg-gray-700 text-white hover:bg-gray-600"
              : "bg-red-600 text-white hover:bg-red-500",
          )}
          title={audioEnabled ? "Mute" : "Unmute"}
        >
          {audioEnabled ? <Mic size={18} /> : <MicOff size={18} />}
        </button>

        <button
          onClick={toggleVideo}
          className={cn(
            "p-3 rounded-full",
            videoEnabled
              ? "bg-gray-700 text-white hover:bg-gray-600"
              : "bg-red-600 text-white hover:bg-red-500",
          )}
          title={videoEnabled ? "Stop video" : "Start video"}
        >
          {videoEnabled ? <Video size={18} /> : <VideoOff size={18} />}
        </button>

        <button
          onClick={toggleScreenShare}
          className={cn(
            "p-3 rounded-full",
            screenSharing
              ? "bg-accent text-white hover:bg-accent/80"
              : "bg-gray-700 text-white hover:bg-gray-600",
          )}
          title={screenSharing ? "Stop sharing" : "Share screen"}
        >
          {screenSharing ? (
            <MonitorOff size={18} />
          ) : (
            <MonitorUp size={18} />
          )}
        </button>

        <button
          onClick={leave}
          className="p-3 rounded-full bg-red-600 text-white hover:bg-red-500"
          title="Leave"
        >
          <PhoneOff size={18} />
        </button>
      </div>
    </div>
  );
}
