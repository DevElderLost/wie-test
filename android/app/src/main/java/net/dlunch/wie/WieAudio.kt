package net.dlunch.wie

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import java.util.concurrent.ConcurrentHashMap

/**
 * Called from Rust (`wie_jni/src/audio.rs` `AndroidAudioSink::play_wave`) via
 * `env.call_method(&self.audio_obj, "playWave", "(II[S)V", ...)`.
 *
 * One `AudioTrack` per (channel, sampleRate) pair, recreated if the sample
 * rate for a channel changes mid-stream (WIPI apps can retarget a channel's
 * rate between plays). Playback is fire-and-forget STREAM mode, matching
 * wie_cli's rodio usage - see wie_cli/src/main.rs `audio_thread`.
 */
class WieAudio {
    private data class TrackKey(val channel: Int, val sampleRate: Int)

    private val tracks = ConcurrentHashMap<TrackKey, AudioTrack>()

    fun playWave(channel: Int, sampleRate: Int, pcm: ShortArray) {
        if (pcm.isEmpty()) return

        val key = TrackKey(channel, sampleRate)
        val track = tracks.getOrPut(key) { createTrack(sampleRate) }

        try {
            track.write(pcm, 0, pcm.size, AudioTrack.WRITE_BLOCKING)
            if (track.playState != AudioTrack.PLAYSTATE_PLAYING) {
                track.play()
            }
        } catch (e: IllegalStateException) {
            // Track went bad (e.g. audio focus loss) - drop it and retry next call.
            tracks.remove(key)
            track.release()
        }
    }

    private fun createTrack(sampleRate: Int): AudioTrack {
        val minBufferSize = AudioTrack.getMinBufferSize(
            sampleRate,
            AudioFormat.CHANNEL_OUT_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        return AudioTrack(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_GAME)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build(),
            AudioFormat.Builder()
                .setSampleRate(sampleRate)
                .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .build(),
            maxOf(minBufferSize, 4096),
            AudioTrack.MODE_STREAM,
            AudioManager_AUDIO_SESSION_ID_GENERATE,
        )
    }

    fun releaseAll() {
        tracks.values.forEach { it.release() }
        tracks.clear()
    }

    companion object {
        // AudioManager.AUDIO_SESSION_ID_GENERATE, inlined to avoid an extra import churn here.
        private const val AudioManager_AUDIO_SESSION_ID_GENERATE = 0

        @Volatile
        private var instance: WieAudio? = null

        @JvmStatic
        fun getInstance(): WieAudio =
            instance ?: synchronized(this) {
                instance ?: WieAudio().also { instance = it }
            }
    }
}
