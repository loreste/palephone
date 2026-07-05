export interface MediaRuntimeFacts {
  hasMediaDevices: boolean;
  hasGetUserMedia: boolean;
  hasCanvasCaptureStream: boolean;
  hasOffscreenCanvas: boolean;
  hasRequestVideoFrameCallback: boolean;
  hasMediaStreamTrackProcessor: boolean;
  hasMediaStreamTrackGenerator: boolean;
  audioConstraints?: MediaTrackSupportedConstraints;
}

export interface MediaRuntimeCapabilities {
  echoCancellation: boolean;
  noiseSuppression: boolean;
  autoGainControl: boolean;
  virtualBackgrounds: boolean;
  backgroundBlur: boolean;
  insertableStreams: boolean;
  blockers: string[];
}

export function collectMediaRuntimeFacts(): MediaRuntimeFacts {
  const audioConstraints = navigator.mediaDevices?.getSupportedConstraints?.() ?? {};
  const canvas = typeof document !== "undefined" ? document.createElement("canvas") : null;
  const video = typeof document !== "undefined" ? document.createElement("video") : null;
  return {
    hasMediaDevices: Boolean(navigator.mediaDevices),
    hasGetUserMedia: Boolean(navigator.mediaDevices?.getUserMedia),
    hasCanvasCaptureStream: Boolean(canvas && "captureStream" in canvas),
    hasOffscreenCanvas: typeof OffscreenCanvas !== "undefined",
    hasRequestVideoFrameCallback: Boolean(video && "requestVideoFrameCallback" in video),
    hasMediaStreamTrackProcessor: typeof (globalThis as any).MediaStreamTrackProcessor !== "undefined",
    hasMediaStreamTrackGenerator: typeof (globalThis as any).MediaStreamTrackGenerator !== "undefined",
    audioConstraints,
  };
}

export function evaluateMediaRuntimeCapabilities(facts: MediaRuntimeFacts): MediaRuntimeCapabilities {
  const blockers: string[] = [];
  if (!facts.hasMediaDevices || !facts.hasGetUserMedia) {
    blockers.push("browser_media_capture_unavailable");
  }

  const echoCancellation = Boolean(facts.audioConstraints?.echoCancellation);
  const noiseSuppression = Boolean(facts.audioConstraints?.noiseSuppression);
  const autoGainControl = Boolean(facts.audioConstraints?.autoGainControl);
  if (!noiseSuppression) {
    blockers.push("noise_suppression_constraint_unavailable");
  }

  const insertableStreams = facts.hasMediaStreamTrackProcessor && facts.hasMediaStreamTrackGenerator;
  const framePipeline = facts.hasCanvasCaptureStream && (facts.hasOffscreenCanvas || facts.hasRequestVideoFrameCallback);
  const virtualBackgrounds = facts.hasGetUserMedia && framePipeline && insertableStreams;
  const backgroundBlur = facts.hasGetUserMedia && framePipeline;

  if (!facts.hasCanvasCaptureStream) {
    blockers.push("canvas_capture_stream_unavailable");
  }
  if (!facts.hasOffscreenCanvas && !facts.hasRequestVideoFrameCallback) {
    blockers.push("video_frame_processing_unavailable");
  }
  if (!insertableStreams) {
    blockers.push("insertable_streams_unavailable");
  }

  return {
    echoCancellation,
    noiseSuppression,
    autoGainControl,
    virtualBackgrounds,
    backgroundBlur,
    insertableStreams,
    blockers,
  };
}

export function currentMediaRuntimeCapabilities(): MediaRuntimeCapabilities {
  return evaluateMediaRuntimeCapabilities(collectMediaRuntimeFacts());
}
