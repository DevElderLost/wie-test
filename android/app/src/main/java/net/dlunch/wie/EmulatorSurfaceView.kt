package net.dlunch.wie

import android.content.Context
import android.util.AttributeSet
import android.view.SurfaceHolder
import android.view.SurfaceView

/**
 * Hands its Surface to the native renderer (`AndroidScreen` in
 * wie_jni/src/screen.rs) on creation, and unbinds it on destroy so Rust
 * doesn't hold a dangling ANativeWindow while the app is backgrounded.
 */
class EmulatorSurfaceView(context: Context, attrs: AttributeSet? = null) : SurfaceView(context, attrs), SurfaceHolder.Callback {

    init {
        holder.addCallback(this)
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        WieNative.nativeSurfaceCreated(holder.surface)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
        // AndroidScreen currently hardcodes the emulated screen size
        // (SCREEN_WIDTH/SCREEN_HEIGHT in wie_jni/src/lib.rs) and just blits
        // into whatever geometry it set on the surface; Android scales the
        // SurfaceView's buffer to fit this view's layout size for us, so
        // there's nothing to do here for now. If you switch to
        // aspect-ratio-aware letterboxing, recalculate that here.
    }

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        WieNative.nativeSurfaceDestroyed()
    }
}
