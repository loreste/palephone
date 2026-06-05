/*
 * pjsip-sys wrapper header for bindgen.
 *
 * We use the high-level pjsua C API which wraps all PJSIP functionality
 * (registration, calls, media, transports) in a simpler interface.
 */

#include <pjsua-lib/pjsua.h>

/* Tone generator for DTMF feedback */
#include <pjmedia/tonegen.h>

/* Audio device API for device enumeration */
#include <pjmedia-audiodev/audiodev.h>

/* Video device API */
#include <pjmedia-videodev/videodev.h>
