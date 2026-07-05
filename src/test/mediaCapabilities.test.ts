import { describe, expect, it } from "vitest";
import { evaluateMediaRuntimeCapabilities, type MediaRuntimeFacts } from "@/lib/mediaCapabilities";

const baseFacts: MediaRuntimeFacts = {
  hasMediaDevices: true,
  hasGetUserMedia: true,
  hasCanvasCaptureStream: true,
  hasOffscreenCanvas: true,
  hasRequestVideoFrameCallback: false,
  hasMediaStreamTrackProcessor: true,
  hasMediaStreamTrackGenerator: true,
  audioConstraints: {
    echoCancellation: true,
    noiseSuppression: true,
    autoGainControl: true,
  },
};

describe("media runtime capability evaluation", () => {
  it("marks full browser media effects available when required primitives exist", () => {
    const result = evaluateMediaRuntimeCapabilities(baseFacts);
    expect(result.noiseSuppression).toBe(true);
    expect(result.virtualBackgrounds).toBe(true);
    expect(result.backgroundBlur).toBe(true);
    expect(result.insertableStreams).toBe(true);
    expect(result.blockers).toEqual([]);
  });

  it("allows blur without insertable streams but blocks full virtual backgrounds", () => {
    const result = evaluateMediaRuntimeCapabilities({
      ...baseFacts,
      hasMediaStreamTrackProcessor: false,
      hasMediaStreamTrackGenerator: false,
    });
    expect(result.backgroundBlur).toBe(true);
    expect(result.virtualBackgrounds).toBe(false);
    expect(result.blockers).toContain("insertable_streams_unavailable");
  });

  it("does not claim media effects when browser capture is unavailable", () => {
    const result = evaluateMediaRuntimeCapabilities({
      ...baseFacts,
      hasMediaDevices: false,
      hasGetUserMedia: false,
      hasCanvasCaptureStream: false,
      hasOffscreenCanvas: false,
      hasRequestVideoFrameCallback: false,
      audioConstraints: {},
    });
    expect(result.noiseSuppression).toBe(false);
    expect(result.backgroundBlur).toBe(false);
    expect(result.virtualBackgrounds).toBe(false);
    expect(result.blockers).toEqual(
      expect.arrayContaining([
        "browser_media_capture_unavailable",
        "noise_suppression_constraint_unavailable",
        "canvas_capture_stream_unavailable",
        "video_frame_processing_unavailable",
      ]),
    );
  });
});
