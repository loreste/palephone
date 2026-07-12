package com.pale.softphone

import android.app.Activity
import android.util.Log

/**
 * JNI entry points for PJSIP Android media.
 *
 * Called early from the main Activity so the native library's ClassLoader
 * cache and JavaVM pointer are ready before the PJSIP worker initializes
 * the Android camera video factory.
 */
object PaleJni {
    private const val TAG = "PaleJni"

    init {
        try {
            System.loadLibrary("pale_lib")
        } catch (t: Throwable) {
            // Tauri also loads pale_lib; a second load is fine / may no-op.
            Log.d(TAG, "loadLibrary: ${t.message}")
        }
    }

    @JvmStatic
    external fun nativePrepareVideoBackend()

    @JvmStatic
    fun prepare(activity: Activity) {
        try {
            // Force org.pjsip.* onto the compile/runtime classpath so R8 cannot
            // strip camera classes that are only invoked via JNI.
            try {
                Class.forName("org.pjsip.PjCamera2")
                Class.forName("org.pjsip.PjCameraInfo2")
            } catch (_: Throwable) {
                Log.w(TAG, "org.pjsip camera classes not on classpath yet")
            }
            nativePrepareVideoBackend()
            PaleVideoOverlay.ensureAttached(activity)
            Log.i(TAG, "PJSIP video backend prepared")
        } catch (t: Throwable) {
            Log.e(TAG, "prepare failed", t)
        }
    }
}
