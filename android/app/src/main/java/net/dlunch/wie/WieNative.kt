package net.dlunch.wie

import android.view.Surface

/**
 * Thin JNI binding to `wie_jni`. Every `external fun` here must have a
 * matching `Java_net_dlunch_wie_WieNative_native*` symbol in
 * wie_jni/src/lib.rs - keep the two in sync.
 */
object WieNative {
    init {
        System.loadLibrary("wie_jni")
        nativeInitLogging()
    }

    // Key codes used with nativeKeyDown/Up. Must match `convert_key_code` in
    // wie_jni/src/lib.rs exactly (order matters, these are plain ints).
    object KeyCode {
        const val UP = 0
        const val DOWN = 1
        const val LEFT = 2
        const val RIGHT = 3
        const val OK = 4
        const val LEFT_SOFT_KEY = 5
        const val RIGHT_SOFT_KEY = 6
        const val CLEAR = 7
        const val CALL = 8
        const val HANGUP = 9
        const val VOLUME_UP = 10
        const val VOLUME_DOWN = 11
        const val NUM0 = 12
        const val NUM1 = 13
        const val NUM2 = 14
        const val NUM3 = 15
        const val NUM4 = 16
        const val NUM5 = 17
        const val NUM6 = 18
        const val NUM7 = 19
        const val NUM8 = 20
        const val NUM9 = 21
        const val HASH = 22
        const val STAR = 23
    }

    private external fun nativeInitLogging()

    /** [filesDir] should be `context.filesDir.absolutePath`. Returns true on success. */
    external fun nativeLoadApp(filesDir: String, filename: String, data: ByteArray): Boolean

    external fun nativeStart()
    external fun nativeStop()
    external fun nativeDestroy()

    external fun nativeSurfaceCreated(surface: Surface)
    external fun nativeSurfaceDestroyed()

    external fun nativeKeyDown(keyCode: Int)
    external fun nativeKeyUp(keyCode: Int)
}
