//! Android video window binding: Surface → ANativeWindow → pjsua_vid_win.
//!
//! Requires `PaleVideoOverlay` surfaces and `PaleJni.prepare()` on the UI thread.

#![cfg(target_os = "android")]

use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use jni::objects::{JObject, JValue};
use jni::sys::{jobject, JNIEnv as SysJNIEnv};
use jni::JNIEnv;

use crate::android_jni;

// ANativeWindow* owned here; accessed only under careful single-writer patterns
// from the PJSIP worker after UI surfaces are shown.
static REMOTE_WINDOW: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static LOCAL_WINDOW: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static PREVIEW_ACTIVE: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn ANativeWindow_fromSurface(env: *mut SysJNIEnv, surface: jobject) -> *mut c_void;
    fn ANativeWindow_release(window: *mut c_void);
    fn pj_jni_attach_jvm(jni_env: *mut *mut c_void) -> i32;
    fn pj_jni_detach_jvm(attached: i32);
}

fn with_env<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut JNIEnv<'_>) -> Option<R>,
{
    // Prefer attaching via PJSIP helper so we share the same JVM pointer.
    let mut raw_env: *mut c_void = std::ptr::null_mut();
    let attached = unsafe { pj_jni_attach_jvm(&mut raw_env) };
    if raw_env.is_null() {
        return None;
    }
    let result = unsafe {
        let mut env = JNIEnv::from_raw(raw_env as *mut SysJNIEnv).ok()?;
        f(&mut env)
    };
    unsafe {
        pj_jni_detach_jvm(attached);
    }
    result
}

fn surface_to_native_window(env: &mut JNIEnv<'_>, surface: JObject<'_>) -> Option<*mut c_void> {
    if surface.is_null() {
        return None;
    }
    let win = unsafe {
        ANativeWindow_fromSurface(env.get_raw(), surface.as_raw())
    };
    if win.is_null() {
        None
    } else {
        Some(win)
    }
}

fn get_overlay_surface(env: &mut JNIEnv<'_>, method: &str) -> Option<*mut c_void> {
    let cls = env
        .find_class("com/pale/softphone/PaleVideoOverlay")
        .ok()?;
    let surface = env
        .call_static_method(cls, method, "()Landroid/view/Surface;", &[])
        .ok()?
        .l()
        .ok()?;
    surface_to_native_window(env, surface)
}

/// Show/hide overlay views (UI thread work done inside Kotlin).
pub fn set_overlays_visible(remote: bool, local: bool) {
    let _ = with_env(|env| {
        let cls = env
            .find_class("com/pale/softphone/PaleVideoOverlay")
            .ok()?;
        // Use Application context activity via PaleJni helpers if available;
        // PaleVideoOverlay methods that take Activity are only for ensureAttached.
        // Visibility methods without Activity already use stored views.
        let _ = env.call_static_method(
            &cls,
            "setRemoteVisibleNoActivity",
            "(Z)V",
            &[JValue::Bool(remote as u8)],
        );
        let _ = env.call_static_method(
            &cls,
            "setLocalVisibleNoActivity",
            "(Z)V",
            &[JValue::Bool(local as u8)],
        );
        Some(())
    });
}

fn store_window(slot: &AtomicPtr<c_void>, new: Option<*mut c_void>) {
    let prev = slot.swap(
        new.unwrap_or(std::ptr::null_mut()),
        Ordering::SeqCst,
    );
    if !prev.is_null() && Some(prev) != new {
        unsafe { ANativeWindow_release(prev) };
    }
}

fn refresh_windows() {
    let remote = with_env(|env| get_overlay_surface(env, "getRemoteSurface"));
    let local = with_env(|env| get_overlay_surface(env, "getLocalSurface"));
    store_window(&REMOTE_WINDOW, remote);
    store_window(&LOCAL_WINDOW, local);
}

fn hwnd_from_window(win: *mut c_void) -> pjsip_sys::pjmedia_vid_dev_hwnd {
    let mut hwnd = pjsip_sys::pjmedia_vid_dev_hwnd::default();
    hwnd.type_ = pjsip_sys::pjmedia_vid_dev_hwnd_type_PJMEDIA_VID_DEV_HWND_TYPE_ANDROID;
    hwnd.info.android.window = win;
    hwnd
}

/// Bind remote pjsua video window to the overlay Surface, start local preview.
pub unsafe fn bind_call_video(
    call_id: pjsip_sys::pjsua_call_id,
    has_incoming: bool,
    has_outgoing: bool,
) {
    if !android_jni::ensure_pjsip_jvm() {
        log::warn!("android_video: JVM not ready, skip window bind");
        return;
    }

    set_overlays_visible(has_incoming, has_outgoing);
    // Give the UI thread a brief moment; surfaces may become valid after VISIBLE.
    refresh_windows();

    if has_incoming {
        let mut ci: pjsip_sys::pjsua_call_info = std::mem::zeroed();
        if pjsip_sys::pjsua_call_get_info(call_id, &mut ci) != 0 {
            return;
        }
        let remote_win = REMOTE_WINDOW.load(Ordering::SeqCst);
        if !remote_win.is_null() {
            for i in 0..ci.media_cnt as usize {
                let media = &ci.media[i];
                if media.type_ != 2 {
                    continue;
                }
                let win_id = media.stream.vid.win_in;
                if win_id < 0 {
                    continue;
                }
                let hwnd = hwnd_from_window(remote_win);
                let st = pjsip_sys::pjsua_vid_win_set_win(win_id, &hwnd);
                let _ = pjsip_sys::pjsua_vid_win_set_show(win_id, 1);
                log::info!(
                    "android_video: bound remote win_id={win_id} to Surface (status={st})"
                );
            }
        } else {
            log::warn!("android_video: remote Surface not ready yet");
        }
    }

    if has_outgoing {
        start_local_preview();
    } else {
        stop_local_preview();
    }
}

pub fn start_local_preview() {
    if PREVIEW_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    refresh_windows();
    let win = LOCAL_WINDOW.load(Ordering::SeqCst);
    if win.is_null() {
        log::warn!("android_video: local Surface not ready for preview");
        return;
    }

    let st = unsafe {
        let mut param = pjsip_sys::pjsua_vid_preview_param::default();
        pjsip_sys::pjsua_vid_preview_param_default(&mut param);
        param.show = 1;
        param.wnd = hwnd_from_window(win);
        // Device 0 is typically first capture device (Back camera after enum).
        let cap_dev: pjsip_sys::pjmedia_vid_dev_index = 0;
        pjsip_sys::pjsua_vid_preview_start(cap_dev, &param)
    };
    if st == 0 {
        PREVIEW_ACTIVE.store(true, Ordering::SeqCst);
        log::info!("android_video: local preview started");
    } else {
        log::warn!("android_video: pjsua_vid_preview_start failed status={st}");
    }
}

pub fn stop_local_preview() {
    if !PREVIEW_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let cap_dev: pjsip_sys::pjmedia_vid_dev_index = 0;
    unsafe {
        let _ = pjsip_sys::pjsua_vid_preview_stop(cap_dev);
    }
    PREVIEW_ACTIVE.store(false, Ordering::SeqCst);
    log::info!("android_video: local preview stopped");
}
