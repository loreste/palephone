package com.pale.softphone

import android.app.Activity
import android.graphics.Color
import android.graphics.PixelFormat
import android.os.Build
import android.util.Log
import android.view.Gravity
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout

/**
 * Lightweight video surface overlay hosted above the Tauri WebView.
 *
 * PJSIP Android OpenGL renderer needs an [android.view.Surface] /
 * ANativeWindow. The webview cannot supply that, so we add a native
 * SurfaceView when video is active and expose its Surface to Rust via JNI.
 *
 * Layout (portrait defaults; adjustable from Rust later):
 * - remote video: full-bleed under status bar, above WebView when shown
 * - local preview: small PIP bottom-right
 */
object PaleVideoOverlay {
    private const val TAG = "PaleVideoOverlay"

    @Volatile
    private var root: FrameLayout? = null

    @Volatile
    private var remoteView: SurfaceView? = null

    @Volatile
    private var localView: SurfaceView? = null

    @Volatile
    private var remoteSurface: Surface? = null

    @Volatile
    private var localSurface: Surface? = null

    @JvmStatic
    fun ensureAttached(activity: Activity) {
        if (root != null) return
        activity.runOnUiThread {
            if (root != null) return@runOnUiThread
            val content = activity.findViewById<ViewGroup>(android.R.id.content)
            val overlay = FrameLayout(activity).apply {
                layoutParams = FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.MATCH_PARENT
                )
                // Transparent so WebView UI remains interactive where not covered
                setBackgroundColor(Color.TRANSPARENT)
                // Don't intercept touches by default — only video surfaces consume them
                isClickable = false
                isFocusable = false
            }

            val remote = SurfaceView(activity).apply {
                holder.setFormat(PixelFormat.TRANSLUCENT)
                holder.addCallback(object : SurfaceHolder.Callback {
                    override fun surfaceCreated(holder: SurfaceHolder) {
                        remoteSurface = holder.surface
                        Log.i(TAG, "remote surface created")
                        notifySurfaceChanged(true, true)
                    }
                    override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {}
                    override fun surfaceDestroyed(holder: SurfaceHolder) {
                        remoteSurface = null
                        Log.i(TAG, "remote surface destroyed")
                        notifySurfaceChanged(true, false)
                    }
                })
                visibility = View.GONE
                layoutParams = FrameLayout.LayoutParams(
                    ViewGroup.LayoutParams.MATCH_PARENT,
                    ViewGroup.LayoutParams.MATCH_PARENT
                )
            }

            val density = activity.resources.displayMetrics.density
            val pipW = (120 * density).toInt()
            val pipH = (160 * density).toInt()
            val margin = (16 * density).toInt()
            val local = SurfaceView(activity).apply {
                holder.addCallback(object : SurfaceHolder.Callback {
                    override fun surfaceCreated(holder: SurfaceHolder) {
                        localSurface = holder.surface
                        Log.i(TAG, "local surface created")
                        notifySurfaceChanged(false, true)
                    }
                    override fun surfaceChanged(holder: SurfaceHolder, format: Int, w: Int, h: Int) {}
                    override fun surfaceDestroyed(holder: SurfaceHolder) {
                        localSurface = null
                        Log.i(TAG, "local surface destroyed")
                        notifySurfaceChanged(false, false)
                    }
                })
                visibility = View.GONE
                layoutParams = FrameLayout.LayoutParams(pipW, pipH, Gravity.BOTTOM or Gravity.END).apply {
                    setMargins(margin, margin, margin, margin)
                }
                // Ensure PIP draws above remote
                elevation = 8f
            }

            overlay.addView(remote)
            overlay.addView(local)
            content.addView(overlay)

            root = overlay
            remoteView = remote
            localView = local
            Log.i(TAG, "video overlay attached")
        }
    }

    @JvmStatic
    fun setRemoteVisible(activity: Activity, visible: Boolean) {
        ensureAttached(activity)
        activity.runOnUiThread {
            remoteView?.visibility = if (visible) View.VISIBLE else View.GONE
            // When remote is visible, let the surface receive layout
            root?.bringToFront()
        }
    }

    @JvmStatic
    fun setLocalVisible(activity: Activity, visible: Boolean) {
        ensureAttached(activity)
        activity.runOnUiThread {
            localView?.visibility = if (visible) View.VISIBLE else View.GONE
            root?.bringToFront()
        }
    }

    @JvmStatic
    fun getRemoteSurface(): Surface? = remoteSurface

    @JvmStatic
    fun getLocalSurface(): Surface? = localSurface

    @JvmStatic
    fun getRemoteSurfaceView(): SurfaceView? = remoteView

    @JvmStatic
    fun getLocalSurfaceView(): SurfaceView? = localView

    // Native callback implemented in Rust when video window is wired.
    @JvmStatic
    private external fun nativeOnSurfaceChanged(isRemote: Boolean, available: Boolean)

    private fun notifySurfaceChanged(isRemote: Boolean, available: Boolean) {
        try {
            nativeOnSurfaceChanged(isRemote, available)
        } catch (t: Throwable) {
            // Native method may not be registered until engine starts.
            Log.d(TAG, "nativeOnSurfaceChanged skipped: ${t.message}")
        }
    }
}
