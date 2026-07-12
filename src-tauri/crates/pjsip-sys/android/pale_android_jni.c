/*
 * Pale Android JNI glue for PJSIP.
 *
 * 1) Owns JNI_OnLoad so we can register the JavaVM with PJSIP (pj_jni_set_jvm)
 *    without fighting Tauri for the single OnLoad symbol in the shared library.
 * 2) Caches the application ClassLoader so PJSIP's android_dev.c can resolve
 *    org.pjsip.* classes from the pjsip worker thread (FindClass alone only
 *    works on the system ClassLoader from non-main threads).
 *
 * Build: only linked on target_os = android (see build.rs).
 */

#include <jni.h>
#include <string.h>
#include <android/log.h>

#define LOG_TAG "PaleAndroidJni"
#define ALOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define ALOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

/* From pjlib (pjsip-sys). */
extern void pj_jni_set_jvm(void *jvm);

static JavaVM *g_vm = NULL;
static jobject g_class_loader = NULL;
static jmethodID g_load_class = NULL;

static int cache_class_loader(JNIEnv *env)
{
    /* Use a class we ship in the APK to obtain the app ClassLoader. */
    jclass pale = (*env)->FindClass(env, "com/pale/softphone/PaleJni");
    if (!pale || (*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        ALOGE("FindClass(com/pale/softphone/PaleJni) failed in JNI_OnLoad");
        return -1;
    }

    jclass class_class = (*env)->GetObjectClass(env, pale);
    jmethodID get_cl = (*env)->GetMethodID(
        env, class_class, "getClassLoader", "()Ljava/lang/ClassLoader;");
    if (!get_cl) {
        ALOGE("getClassLoader method missing");
        return -1;
    }

    jobject cl = (*env)->CallObjectMethod(env, pale, get_cl);
    if (!cl || (*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        ALOGE("getClassLoader() returned null");
        return -1;
    }

    g_class_loader = (*env)->NewGlobalRef(env, cl);
    (*env)->DeleteLocalRef(env, cl);

    jclass cl_cls = (*env)->GetObjectClass(env, g_class_loader);
    g_load_class = (*env)->GetMethodID(
        env, cl_cls, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;");
    if (!g_load_class) {
        ALOGE("ClassLoader.loadClass missing");
        return -1;
    }

    ALOGI("Cached application ClassLoader for PJSIP JNI");
    return 0;
}

/**
 * Class lookup that works from any thread once JNI_OnLoad has run.
 * `slash_name` is JNI form, e.g. "org/pjsip/PjCamera2".
 * Returns a local ref, or NULL on failure (exception cleared).
 */
jclass pale_android_find_class(JNIEnv *env, const char *slash_name)
{
    if (!env || !slash_name)
        return NULL;

    /* Prefer ClassLoader path for app classes. */
    if (g_class_loader && g_load_class) {
        char dotted[256];
        size_t n = strlen(slash_name);
        if (n >= sizeof(dotted))
            return NULL;
        for (size_t i = 0; i <= n; i++) {
            char c = slash_name[i];
            dotted[i] = (c == '/') ? '.' : c;
        }
        jstring jname = (*env)->NewStringUTF(env, dotted);
        if (!jname)
            return NULL;
        jclass cls = (jclass)(*env)->CallObjectMethod(
            env, g_class_loader, g_load_class, jname);
        (*env)->DeleteLocalRef(env, jname);
        if ((*env)->ExceptionCheck(env)) {
            (*env)->ExceptionClear(env);
            return NULL;
        }
        return cls;
    }

    /* Fallback (main/load thread only for app classes). */
    jclass cls = (*env)->FindClass(env, slash_name);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
        return NULL;
    }
    return cls;
}

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved)
{
    (void)reserved;
    g_vm = vm;
    pj_jni_set_jvm((void *)vm);

    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        ALOGE("GetEnv failed in JNI_OnLoad");
        return JNI_VERSION_1_6;
    }

    if (cache_class_loader(env) != 0) {
        ALOGE("ClassLoader cache failed — PJSIP video may not find org.pjsip.*");
    }

    return JNI_VERSION_1_6;
}

JNIEXPORT void JNICALL
Java_com_pale_softphone_PaleJni_nativePrepareVideoBackend(JNIEnv *env, jclass clazz)
{
    (void)clazz;
    /* Re-cache loader if OnLoad ran before PaleJni was available. */
    if (!g_class_loader) {
        cache_class_loader(env);
    }
    if (g_vm) {
        pj_jni_set_jvm((void *)g_vm);
    }
    ALOGI("nativePrepareVideoBackend done");
}

/* Rust readiness probe (pale-core android_jni). */
int pale_android_jvm_ready(void)
{
    return g_vm != NULL ? 1 : 0;
}
