mod audio;
mod filelog;
mod screen;
mod storage;

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteArray, JClass, JObject, JString},
    sys::{jboolean, jint, JNI_TRUE},
};

use wie_backend::{Emulator, Event, KeyCode, Options, extract_zip};
use wie_j2me::J2MEEmulator;
use wie_ktf::KtfEmulator;
use wie_lgt::LgtEmulator;
use wie_skt::SktEmulator;

use crate::{audio::AndroidAudioSink, screen::AndroidScreen, storage::{AndroidDatabaseRepository, AndroidFilesystem}};

// Old KTF/LGT/SKT WIPI handsets were almost universally 176x220 (QCIF+) or
// 240x320 (QVGA). We hardcode 240x320 for the first cut, same as wie_cli
// does - see wie_cli/src/main.rs `start()`. TODO: read the real size from
// the loaded app's descriptor once we plumb that through, and let
// EmulatorSurfaceView letterbox to it instead of hardcoding both ends.
const SCREEN_WIDTH: u32 = 240;
const SCREEN_HEIGHT: u32 = 320;

struct AndroidPlatform {
    screen: Arc<AndroidScreen>,
    database_repository: AndroidDatabaseRepository,
    filesystem: AndroidFilesystem,
    vm: JavaVM,
    audio_obj: GlobalRef,
    vibrator_obj: Option<GlobalRef>,
}

impl wie_backend::Platform for AndroidPlatform {
    fn screen(&self) -> &dyn wie_backend::Screen {
        self.screen.as_ref()
    }

    fn now(&self) -> wie_backend::Instant {
        let since_epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        wie_backend::Instant::from_epoch_millis(since_epoch.as_millis() as _)
    }

    fn database_repository(&self) -> &dyn wie_backend::DatabaseRepository {
        &self.database_repository
    }

    fn filesystem(&self) -> &dyn wie_backend::Filesystem {
        &self.filesystem
    }

    fn audio_sink(&self) -> Box<dyn wie_backend::AudioSink> {
        // JavaVM doesn't impl Clone, but attaching from multiple threads to
        // the same VM is fine and is exactly what this is for.
        let vm = unsafe { JavaVM::from_raw(self.vm.get_java_vm_pointer()).unwrap() };
        Box::new(AndroidAudioSink::new(vm, self.audio_obj.clone()))
    }

    fn write_stdout(&self, buf: &[u8]) {
        if let Ok(s) = std::str::from_utf8(buf) {
            log::info!(target: "wie_stdout", "{s}");
        }
    }

    fn write_stderr(&self, buf: &[u8]) {
        if let Ok(s) = std::str::from_utf8(buf) {
            log::warn!(target: "wie_stderr", "{s}");
        }
    }

    fn exit(&self) {
        RUNNING.store(false, Ordering::SeqCst);
    }

    fn vibrate(&self, duration_ms: u64, _intensity: u8) {
        let Some(vibrator) = &self.vibrator_obj else { return };
        if let Ok(mut env) = self.vm.attach_current_thread() {
            let _ = env.call_method(vibrator, "vibrate", "(J)V", &[jni::objects::JValue::Long(duration_ms as i64)]);
        }
    }
}

unsafe impl Send for AndroidPlatform {}
unsafe impl Sync for AndroidPlatform {}

struct EmulatorState {
    emulator: Box<dyn Emulator + Send>,
}

// Global emulator + tick-thread state. A real app should probably wrap this
// more carefully, but a single global slot matches wie_jni's job: there's
// only ever one emulator instance alive per process, matching one Activity.
static STATE: Mutex<Option<EmulatorState>> = Mutex::new(None);
static RUNNING: AtomicBool = AtomicBool::new(false);
// Shared with the AndroidPlatform's `screen` field so nativeSurfaceCreated /
// nativeSurfaceDestroyed (which don't have access to the type-erased
// `Box<dyn Emulator>` in STATE) can still bind/unbind the ANativeWindow.
static SCREEN: Mutex<Option<Arc<AndroidScreen>>> = Mutex::new(None);
// The SurfaceView's Surface is typically created (surfaceCreated ->
// nativeSurfaceCreated) as soon as the view attaches - well before the user
// has picked a ROM and nativeLoadApp has run. We can't bind it to the
// AndroidScreen yet at that point (SCREEN is still empty), so stash it here
// and nativeLoadApp claims it once the screen exists. Without this, the
// window is silently dropped and the game runs with nothing to draw into
// (black screen, controls still visible/responsive).
static PENDING_WINDOW: Mutex<Option<ndk::native_window::NativeWindow>> = Mutex::new(None);

fn jstring_to_string(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|s| s.into()).unwrap_or_default()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeInitLogging(mut env: JNIEnv, _class: JClass, log_path: JString) {
    let log_path = jstring_to_string(&mut env, &log_path);
    filelog::init(&log_path);
    log::info!("wie_jni logging initialized");
}

/// Loads a rom/app archive (.zip for KTF/LGT/SKT, or a jar with an
/// accompanying jad) and starts the emulator. Blocks until either a fatal
/// load error or the emulator is ready; the actual tick loop is started
/// separately via `nativeStart` once the surface exists.
///
/// Returns true on success. On failure, check logcat (tag `wie_jni`) - we
/// don't currently propagate a structured error back to Kotlin.
#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeLoadApp(
    mut env: JNIEnv,
    _class: JClass,
    files_dir: JString,
    filename: JString,
    data: JByteArray,
) -> jboolean {
    let files_dir = jstring_to_string(&mut env, &files_dir);
    let filename = jstring_to_string(&mut env, &filename);

    let data = match env.convert_byte_array(&data) {
        Ok(v) => v,
        Err(e) => {
            log::error!("failed to read rom bytes: {e}");
            return jni::sys::JNI_FALSE;
        }
    };

    let vm = env.get_java_vm().unwrap();

    // WieAudio and Vibrator singletons are expected to be created and
    // passed down before this call (see WieNative.kt nativeInit); for the
    // first cut we just re-resolve them via a static Kotlin accessor to
    // keep this function's signature simple. Replace with GlobalRefs
    // passed explicitly if you need per-instance audio (e.g. multiple
    // emulator windows).
    let audio_class = env.find_class("net/dlunch/wie/WieAudio").unwrap();
    let audio_obj = env
        .call_static_method(&audio_class, "getInstance", "()Lnet/dlunch/wie/WieAudio;", &[])
        .and_then(|v| v.l())
        .and_then(|o| env.new_global_ref(o))
        .unwrap();

    let base_path = PathBuf::from(&files_dir).join("wie");
    let screen = Arc::new(AndroidScreen::new(SCREEN_WIDTH, SCREEN_HEIGHT));
    *SCREEN.lock().unwrap() = Some(screen.clone());
    if let Some(window) = PENDING_WINDOW.lock().unwrap().take() {
        screen.set_window(Some(window));
    }

    let platform = Box::new(AndroidPlatform {
        screen,
        database_repository: AndroidDatabaseRepository::new(base_path.clone()),
        filesystem: AndroidFilesystem::new(base_path),
        vm,
        audio_obj,
        vibrator_obj: None, // TODO wire up net.dlunch.wie.WieVibrator similarly to WieAudio if you want haptics
    });

    let options = Options {
        enable_gdbserver: false,
        profile: None,
    };

    let emulator: anyhow::Result<Box<dyn Emulator + Send>> = (|| {
        if filename.ends_with(".zip") {
            let files = extract_zip(&data).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            if KtfEmulator::loadable_archive(&files) {
                Ok(Box::new(KtfEmulator::from_archive(platform, files, options).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            } else if LgtEmulator::loadable_archive(&files) {
                Ok(Box::new(LgtEmulator::from_archive(platform, files, options).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            } else if SktEmulator::loadable_archive(&files) {
                Ok(Box::new(SktEmulator::from_archive(platform, files).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            } else {
                anyhow::bail!("Unknown archive format")
            }
        } else if filename.ends_with(".jar") {
            let name = filename.trim_end_matches(".jar").to_string();
            if KtfEmulator::loadable_jar(&data) {
                Ok(Box::new(KtfEmulator::from_jar(platform, &filename, data, &name, &name, None, options).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            } else if LgtEmulator::loadable_jar(&data) {
                Ok(Box::new(LgtEmulator::from_jar(platform, &filename, data, &name, &name, None, options).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            } else if SktEmulator::loadable_jar(&data) {
                Ok(Box::new(SktEmulator::from_jar(platform, &filename, data, &name, None).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            } else {
                Ok(Box::new(J2MEEmulator::from_jar(platform, &filename, data).map_err(|e| anyhow::anyhow!("{e:?}"))?) as Box<dyn Emulator + Send>)
            }
        } else {
            anyhow::bail!("Unsupported file type: {filename} (expected .zip or .jar)")
        }
    })();

    match emulator {
        Ok(emulator) => {
            log::info!("app loaded OK: {filename}");
            *STATE.lock().unwrap() = Some(EmulatorState { emulator });
            JNI_TRUE
        }
        Err(e) => {
            log::error!("failed to load app: {e:#}");
            jni::sys::JNI_FALSE
        }
    }
}

/// Starts the tick loop on a dedicated thread. Call once after
/// `nativeLoadApp` succeeds and the Surface is ready.
#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeStart(_env: JNIEnv, _class: JClass) {
    log::info!("nativeStart: tick thread starting");
    RUNNING.store(true, Ordering::SeqCst);
    thread::spawn(|| {
        // ~16ms ≈ 60Hz; the original WIPI/MIDP handsets ran their UI loop
        // far slower than this, so this is intentionally generous headroom,
        // not a hard timing requirement copied from real hardware.
        let tick_interval = Duration::from_millis(16);
        while RUNNING.load(Ordering::SeqCst) {
            let mut guard = STATE.lock().unwrap();
            if let Some(state) = guard.as_mut() {
                state.emulator.handle_event(Event::Redraw);
                if let Err(e) = state.emulator.tick() {
                    log::error!("tick error: {e:?}");
                    RUNNING.store(false, Ordering::SeqCst);
                    break;
                }
            } else {
                break;
            }
            drop(guard);
            thread::sleep(tick_interval);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeStop(_env: JNIEnv, _class: JClass) {
    RUNNING.store(false, Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeDestroy(_env: JNIEnv, _class: JClass) {
    RUNNING.store(false, Ordering::SeqCst);
    *STATE.lock().unwrap() = None;
    *SCREEN.lock().unwrap() = None;
    *PENDING_WINDOW.lock().unwrap() = None;
}

/// `surface` is an `android.view.Surface` obtained from
/// `SurfaceHolder.getSurface()` in `EmulatorSurfaceView.surfaceCreated`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeSurfaceCreated(env: JNIEnv, _class: JClass, surface: JObject) {
    let raw_env = env.get_raw();
    let raw_surface = surface.as_raw();

    // SAFETY: `from_surface` requires a valid JNIEnv* and a non-null
    // jobject Surface, both guaranteed by the JVM calling us here.
    let native_window = unsafe { ndk::native_window::NativeWindow::from_surface(raw_env.cast(), raw_surface.cast()) };

    if let Some(screen) = SCREEN.lock().unwrap().as_ref() {
        screen.set_window(native_window);
    } else {
        // ROM not loaded yet (this Surface was created as soon as the
        // SurfaceView attached, well before nativeLoadApp runs) - stash it
        // for nativeLoadApp to bind once the AndroidScreen exists.
        *PENDING_WINDOW.lock().unwrap() = native_window;
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeSurfaceDestroyed(_env: JNIEnv, _class: JClass) {
    *PENDING_WINDOW.lock().unwrap() = None;
    if let Some(screen) = SCREEN.lock().unwrap().as_ref() {
        screen.set_window(None);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeKeyDown(_env: JNIEnv, _class: JClass, key_code: jint) {
    if let Some(code) = convert_key_code(key_code)
        && let Some(state) = STATE.lock().unwrap().as_mut()
    {
        state.emulator.handle_event(Event::Keydown(code));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_net_dlunch_wie_WieNative_nativeKeyUp(_env: JNIEnv, _class: JClass, key_code: jint) {
    if let Some(code) = convert_key_code(key_code)
        && let Some(state) = STATE.lock().unwrap().as_mut()
    {
        state.emulator.handle_event(Event::Keyup(code));
    }
}

/// Matches the `WieKeyCode` IntDef constants in `WieNative.kt`. Kept as
/// plain ints (rather than routing raw Android `KeyEvent.KEYCODE_*`
/// through JNI) so the virtual on-screen keypad and any future physical
/// keyboard/dpad mapping both funnel through one small table on the Kotlin
/// side - see `KeyCode` in wie_backend/src/system.rs for the full legal set.
fn convert_key_code(code: jint) -> Option<KeyCode> {
    Some(match code {
        0 => KeyCode::UP,
        1 => KeyCode::DOWN,
        2 => KeyCode::LEFT,
        3 => KeyCode::RIGHT,
        4 => KeyCode::OK,
        5 => KeyCode::LEFT_SOFT_KEY,
        6 => KeyCode::RIGHT_SOFT_KEY,
        7 => KeyCode::CLEAR,
        8 => KeyCode::CALL,
        9 => KeyCode::HANGUP,
        10 => KeyCode::VOLUME_UP,
        11 => KeyCode::VOLUME_DOWN,
        12 => KeyCode::NUM0,
        13 => KeyCode::NUM1,
        14 => KeyCode::NUM2,
        15 => KeyCode::NUM3,
        16 => KeyCode::NUM4,
        17 => KeyCode::NUM5,
        18 => KeyCode::NUM6,
        19 => KeyCode::NUM7,
        20 => KeyCode::NUM8,
        21 => KeyCode::NUM9,
        22 => KeyCode::HASH,
        23 => KeyCode::STAR,
        _ => return None,
    })
}
