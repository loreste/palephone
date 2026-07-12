//! Android JNI readiness for PJSIP video/audio factories.
//!
//! JVM registration happens in `pjsip-sys/android/pale_android_jni.c`
//! (`JNI_OnLoad` + `PaleJni.nativePrepareVideoBackend`).

#![cfg(target_os = "android")]

use std::sync::atomic::{AtomicBool, Ordering};

static JVM_READY: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn pale_android_jvm_ready() -> i32;
}

/// True when Pale's JNI_OnLoad (or prepare) has stored a JavaVM for PJSIP.
pub fn ensure_pjsip_jvm() -> bool {
    if JVM_READY.load(Ordering::Acquire) {
        return true;
    }
    let ok = unsafe { pale_android_jvm_ready() } != 0;
    if ok {
        JVM_READY.store(true, Ordering::Release);
        log::info!("Android: PJSIP JavaVM is set");
    } else {
        log::warn!(
            "Android: JavaVM not ready yet — call PaleJni.prepare() from MainActivity"
        );
    }
    ok
}
