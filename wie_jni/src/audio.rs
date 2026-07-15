//! We don't try to open an AAudio/OpenSL stream directly from Rust - that's
//! more moving parts than we need. Instead we hand PCM buffers back across
//! JNI to a small Kotlin helper (`WieAudio`) that owns a pool of
//! `android.media.AudioTrack`s, one per emulator "channel" id. This mirrors
//! how wie_cli hands buffers to `rodio` on a dedicated thread - see
//! wie_cli/src/main.rs `audio_thread`.
//!
//! MIDI (program change / note on-off / sysex) is currently a no-op. The
//! Korean carrier games use it sparingly for ringtone-style BGM; wiring up
//! a tiny softsynth (or routing through Android's built-in MIDI framework)
//! is a good follow-up but out of scope for the first working build.

use jni::{
    JavaVM,
    objects::{GlobalRef, JValue},
    sys::jshort,
};

pub struct AndroidAudioSink {
    vm: JavaVM,
    /// GlobalRef to the `WieAudio` singleton object living on the Kotlin side.
    audio_obj: GlobalRef,
}

impl AndroidAudioSink {
    pub fn new(vm: JavaVM, audio_obj: GlobalRef) -> Self {
        Self { vm, audio_obj }
    }
}

impl wie_backend::AudioSink for AndroidAudioSink {
    fn play_wave(&self, channel: u8, sampling_rate: u32, wave_data: &[i16]) {
        let Ok(mut env) = self.vm.attach_current_thread() else { return };

        // jshort == i16, so this cast is just a type-label change, not a value change.
        let jdata: &[jshort] = wave_data;
        let Ok(array) = env.new_short_array(jdata.len() as i32) else { return };
        if env.set_short_array_region(&array, 0, jdata).is_err() {
            return;
        }

        // Must match WieAudio.playWave(channel: Int, sampleRate: Int, pcm: ShortArray)
        // in android/app/src/main/java/net/dlunch/wie/WieAudio.kt
        let _ = env.call_method(
            &self.audio_obj,
            "playWave",
            "(II[S)V",
            &[JValue::Int(channel as i32), JValue::Int(sampling_rate as i32), JValue::Object(&array)],
        );
    }

    fn midi_note_on(&self, _channel_id: u8, _note: u8, _velocity: u8) {
        tracing::trace!("midi_note_on: not yet implemented on Android backend");
    }

    fn midi_note_off(&self, _channel_id: u8, _note: u8, _velocity: u8) {}

    fn midi_program_change(&self, _channel_id: u8, _program: u8) {}

    fn midi_control_change(&self, _channel_id: u8, _control: u8, _value: u8) {}

    fn midi_pitch_bend(&self, _channel_id: u8, _value: u16) {}

    fn midi_sysex(&self, _data: &[u8]) {}
}

unsafe impl Send for AndroidAudioSink {}
unsafe impl Sync for AndroidAudioSink {}
