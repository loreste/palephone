//! Android JNI bootstrap for PJSIP video/audio factories.
//!
//! Rust exports (reliably present in `libpale_lib.so`):
//! - `JNI_OnLoad` — registers JavaVM + ClassLoader cache
//! - `Java_com_pale_softphone_PaleJni_nativePrepareVideoBackend`
//! - `pale_android_find_class` — used by patched PJSIP `android_dev.c`

#![cfg(target_os = "android")]

use std::ffi::{c_char, c_void, CStr};
use std::os::raw::c_int;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Mutex;

use jni::objects::{GlobalRef, JClass, JObject, JValue};
use jni::sys::{jclass, jint, jvalue, JNI_VERSION_1_6, JNIEnv as SysJNIEnv, JavaVM as SysJavaVM};
use jni::{JNIEnv, JavaVM};

static JVM_READY: AtomicBool = AtomicBool::new(false);
static RAW_VM: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

extern "C" {
    fn pj_jni_set_jvm(jvm: *mut c_void);
}

struct ClassLoaderCache {
    loader: GlobalRef,
}

static CLASS_LOADER: Mutex<Option<ClassLoaderCache>> = Mutex::new(None);

fn set_jvm_ptr(raw: *mut SysJavaVM) {
    if raw.is_null() {
        return;
    }
    unsafe {
        pj_jni_set_jvm(raw as *mut c_void);
    }
    RAW_VM.store(raw as *mut c_void, Ordering::Release);
    JVM_READY.store(true, Ordering::Release);
    log::info!("Android: PJSIP JavaVM registered");
}

fn cache_class_loader(env: &mut JNIEnv<'_>) -> jni::errors::Result<()> {
    let pale = env.find_class("com/pale/softphone/PaleJni")?;
    let loader = env
        .call_method(pale, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?
        .l()?;
    if loader.is_null() {
        return Err(jni::errors::Error::NullPtr("ClassLoader"));
    }
    let global = env.new_global_ref(loader)?;
    *CLASS_LOADER.lock().unwrap() = Some(ClassLoaderCache { loader: global });
    log::info!("Android: cached application ClassLoader for PJSIP JNI");
    Ok(())
}

/// True when a JavaVM has been registered for PJSIP.
pub fn ensure_pjsip_jvm() -> bool {
    if JVM_READY.load(Ordering::Acquire) {
        return true;
    }
    let ready = !RAW_VM.load(Ordering::Acquire).is_null();
    if ready {
        JVM_READY.store(true, Ordering::Release);
        log::info!("Android: PJSIP JavaVM is set");
    } else {
        log::warn!(
            "Android: JavaVM not ready yet — call PaleJni.prepare() from MainActivity"
        );
    }
    ready
}

/// C ABI for patched PJSIP `android_dev.c`.
///
/// # Safety
/// `env` must be a valid `JNIEnv*`; `slash_name` a NUL-terminated path like
/// `org/pjsip/PjCamera2`. Returns a local `jclass` or null.
#[no_mangle]
pub unsafe extern "C" fn pale_android_find_class(
    env: *mut SysJNIEnv,
    slash_name: *const c_char,
) -> jclass {
    if env.is_null() || slash_name.is_null() {
        return std::ptr::null_mut();
    }
    let Ok(mut env) = JNIEnv::from_raw(env) else {
        return std::ptr::null_mut();
    };
    let Ok(name) = CStr::from_ptr(slash_name).to_str() else {
        return std::ptr::null_mut();
    };
    let dotted = name.replace('/', ".");

    if let Some(cache) = CLASS_LOADER.lock().ok().and_then(|g| {
        // Clone GlobalRef is not available; re-lock pattern:
        None::<()>
    }) {
        let _ = cache;
    }

    if let Ok(guard) = CLASS_LOADER.lock() {
        if let Some(cache) = guard.as_ref() {
            let loader = cache.loader.as_obj();
            if let (Ok(jname), Ok(mid)) = (
                env.new_string(&dotted),
                env.get_method_id(
                    "java/lang/ClassLoader",
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                ),
            ) {
                let arg = jvalue {
                    l: jname.into_raw(),
                };
                if let Ok(cls_val) = env.call_method_unchecked(
                    loader,
                    mid,
                    jni::signature::ReturnType::Object,
                    &[arg],
                ) {
                    if let Ok(obj) = cls_val.l() {
                        return obj.into_raw();
                    }
                }
                let _ = env.exception_clear();
            }
        }
    }

    match env.find_class(name) {
        Ok(c) => c.as_raw(),
        Err(_) => {
            let _ = env.exception_clear();
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn pale_android_jvm_ready() -> c_int {
    if ensure_pjsip_jvm() {
        1
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn pale_android_force_link() {}

/// JVM entry when `System.loadLibrary("pale_lib")` runs.
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: *mut SysJavaVM, _reserved: *mut c_void) -> jint {
    if vm.is_null() {
        return JNI_VERSION_1_6 as jint;
    }
    set_jvm_ptr(vm);
    if let Ok(java_vm) = unsafe { JavaVM::from_raw(vm) } {
        if let Ok(mut env) = java_vm.get_env() {
            if let Err(e) = cache_class_loader(&mut env) {
                log::warn!("Android: ClassLoader cache failed in JNI_OnLoad: {e}");
            }
        }
    }
    JNI_VERSION_1_6 as jint
}

/// Called from `PaleJni.prepare(activity)` on the UI thread.
#[no_mangle]
pub extern "system" fn Java_com_pale_softphone_PaleJni_nativePrepareVideoBackend<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) {
    if let Ok(vm) = env.get_java_vm() {
        set_jvm_ptr(vm.get_java_vm_pointer());
    }
    if let Err(e) = cache_class_loader(&mut env) {
        log::error!("Android: ClassLoader cache failed in prepare: {e}");
    } else {
        log::info!("Android: nativePrepareVideoBackend done");
    }
}
