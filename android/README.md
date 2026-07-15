# WIE for Android (JNI + Kotlin)

Wraps the existing `wie_backend`/`wie_ktf`/`wie_lgt`/`wie_skt`/`wie_j2me`
Rust crates (unmodified) behind a new `wie_jni` crate, exposed to a small
Kotlin app via JNI. Same shape as `wie_cli`, just with Android-native
Screen/AudioSink/Filesystem/DatabaseRepository implementations instead of
winit/rodio/std-fs-with-ProjectDirs.

## Layout

- `wie_jni/` (workspace root, sibling of `wie_backend` etc.) - the JNI glue crate
  - `lib.rs` - JNI entry points, emulator lifecycle, tick thread
  - `screen.rs` - blits into an `ANativeWindow` from a Kotlin `Surface`
  - `audio.rs` - forwards PCM to Kotlin `AudioTrack`s over JNI; MIDI is a stub
  - `storage.rs` - filesystem + save-record backends, straight port of `wie_cli`'s
- `android/` - Gradle project
  - `app/src/main/java/net/dlunch/wie/WieNative.kt` - `external fun` declarations
  - `app/src/main/java/net/dlunch/wie/WieAudio.kt` - `AudioTrack` pool called from Rust
  - `app/src/main/java/net/dlunch/wie/EmulatorSurfaceView.kt` - binds/unbinds the native window
  - `app/src/main/java/net/dlunch/wie/VirtualKeypadView.kt` - on-screen dpad/numpad
  - `app/src/main/java/net/dlunch/wie/MainActivity.kt` - file picker + lifecycle wiring

## Build prerequisites

```sh
rustup target add aarch64-linux-android armv7-linux-androideabi
cargo install cargo-ndk
export ANDROID_NDK_HOME=/path/to/ndk   # NDK r27 known-good; older r25+ likely fine too
```

Then:

```sh
cd android
gradle assembleDebug   # or use the "Android" GitHub Actions workflow
```

The `cargoBuildAndroid` Gradle task shells out to `cargo ndk` and drops the
resulting `.so`s into `app/src/main/jniLibs/{arm64-v8a,armeabi-v7a}/` before
every build - see `android/app/build.gradle.kts`.

**This has not been compiled or run on-device yet.** It was written by
reading `wie_backend`'s trait definitions and `wie_cli`'s reference
implementation carefully, but the `ndk` crate's `NativeWindow` API in
particular (`screen.rs`) has shifted across versions and needs to be
checked against whatever version Cargo actually resolves. Treat this as a
solid first draft to compile-fix, not a finished port.

## Known gaps / next steps

1. **`ndk` crate API drift** (`wie_jni/src/screen.rs`) - `set_buffers_geometry`
   and the buffer-lock guard's field names need to be checked against the
   resolved `ndk` version (`cargo doc -p ndk --open` after first
   `cargo build` attempt) and adjusted.
2. **No gradlew wrapper jar committed** - the CI workflow uses
   `gradle/actions/setup-gradle` instead. If you want a local `./gradlew`,
   run `gradle wrapper --gradle-version 8.9` once inside `android/`.
3. **Hardcoded 240x320 screen** (`wie_jni/src/lib.rs` `SCREEN_WIDTH/HEIGHT`) -
   real WIPI apps declare their target resolution (176x220 or 240x320); read
   it from the loaded archive instead of guessing, and letterbox in
   `EmulatorSurfaceView` if the two don't match the phone's actual surface size.
4. **MIDI is a no-op stub** (`wie_jni/src/audio.rs`) - `midi_note_on` etc.
   just log and return. wie_cli routes these to a real MIDI output device,
   which doesn't have a direct Android equivalent; a small software synth
   or Android's `MidiManager` virtual device framework would be the way in.
5. **No ROM library / recent files** - `MainActivity` is a single "pick a
   file" button. Fine for testing your Heroes Lore 4 dumps directly, but
   you'll want a proper picker before this is daily-driver usable.
6. **No error surfacing to the UI** - `nativeLoadApp` failures only show up
   in `logcat` (tag `wie_jni`); worth returning a message string instead of
   a bare `bool` once the happy path works.
7. **`minSdk = 26`** picked somewhat arbitrarily for `ANativeWindow`-related
   API availability; can likely be lowered if you need it.

## Input mapping

`WieNative.KeyCode` (Kotlin) and `convert_key_code` (Rust, `wie_jni/src/lib.rs`)
must stay in sync - both are plain hand-maintained integer tables mirroring
`wie_backend::KeyCode`. If you add a physical-keyboard/gamepad path later,
funnel it through the same integer codes rather than inventing a second
mapping.
