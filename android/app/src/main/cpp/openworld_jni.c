/* SPDX-License-Identifier: Apache-2.0 */
/* JNI over include/openworld.h for app.openworld.core.Native.
 *
 * Strings cross as UTF-8 byte arrays, never through NewStringUTF (modified UTF-8).
 * Events call EventSink.onEvent(byte[]) on the thread that pushes frames, or that makes
 * the call (posters_update). An exception
 * thrown by the listener is cleared here so it never unwinds through the library.
 */
#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "openworld.h"

static JavaVM *g_vm;
static jclass g_sink_class;
static jmethodID g_on_event;

typedef struct {
    ow_job *job;
    jobject sink; /* global ref, or NULL */
} JobBox;

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    JNIEnv *env = NULL;
    jclass local;
    (void)reserved;
    g_vm = vm;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return JNI_ERR;
    }
    local = (*env)->FindClass(env, "app/openworld/core/EventSink");
    if (local == NULL) {
        return JNI_ERR;
    }
    g_sink_class = (jclass)(*env)->NewGlobalRef(env, local);
    (*env)->DeleteLocalRef(env, local);
    if (g_sink_class == NULL) {
        return JNI_ERR;
    }
    g_on_event = (*env)->GetMethodID(env, g_sink_class, "onEvent", "([B)V");
    if (g_on_event == NULL) {
        return JNI_ERR;
    }
    return JNI_VERSION_1_6;
}

/* A NUL-terminated copy of a UTF-8 byte array. Free it. */
static char *to_cstr(JNIEnv *env, jbyteArray bytes) {
    jsize n;
    char *out;
    if (bytes == NULL) {
        return NULL;
    }
    n = (*env)->GetArrayLength(env, bytes);
    out = malloc((size_t)n + 1);
    if (out == NULL) {
        return NULL;
    }
    (*env)->GetByteArrayRegion(env, bytes, 0, n, (jbyte *)out);
    out[n] = '\0';
    return out;
}

/* A byte array of a library string. NULL when the string is NULL or memory ran out. */
static jbyteArray to_bytes(JNIEnv *env, const char *text) {
    size_t n;
    jbyteArray out;
    if (text == NULL) {
        return NULL;
    }
    n = strlen(text);
    if (n > (size_t)INT32_MAX) {
        return NULL;
    }
    out = (*env)->NewByteArray(env, (jsize)n);
    if (out == NULL) {
        return NULL;
    }
    (*env)->SetByteArrayRegion(env, out, 0, (jsize)n, (const jbyte *)text);
    return out;
}

/* Library string to byte array, and the library string freed. */
static jbyteArray take_answer(JNIEnv *env, char *answer) {
    jbyteArray out = to_bytes(env, answer);
    ow_string_free(answer);
    return out;
}

static void on_event(const char *json, void *user) {
    JobBox *box = user;
    JNIEnv *env = NULL;
    int attached = 0;
    jint got;
    jbyteArray line;
    if (box == NULL || box->sink == NULL || json == NULL || g_vm == NULL) {
        return;
    }
    got = (*g_vm)->GetEnv(g_vm, (void **)&env, JNI_VERSION_1_6);
    if (got == JNI_EDETACHED) {
        if ((*g_vm)->AttachCurrentThread(g_vm, &env, NULL) != JNI_OK) {
            return;
        }
        attached = 1;
    } else if (got != JNI_OK) {
        return;
    }
    if (!(*env)->ExceptionCheck(env)) {
        line = to_bytes(env, json);
        if (line != NULL) {
            (*env)->CallVoidMethod(env, box->sink, g_on_event, line);
            (*env)->DeleteLocalRef(env, line);
        }
        if ((*env)->ExceptionCheck(env)) {
            (*env)->ExceptionClear(env);
        }
    }
    if (attached) {
        (*g_vm)->DetachCurrentThread(g_vm);
    }
}

static JobBox *box_of(jlong handle) {
    JobBox *box = (JobBox *)(intptr_t)handle;
    return (box != NULL && box->job != NULL) ? box : NULL;
}

static void release_box(JNIEnv *env, JobBox *box) {
    if (box->sink != NULL) {
        (*env)->DeleteGlobalRef(env, box->sink);
    }
    free(box);
}

/* The bytes a direct buffer holds, or -1 when it is not a direct buffer. */
static jlong capacity(JNIEnv *env, jobject buffer, uint8_t **address) {
    *address = buffer != NULL ? (*env)->GetDirectBufferAddress(env, buffer) : NULL;
    if (*address == NULL) {
        return -1;
    }
    return (*env)->GetDirectBufferCapacity(env, buffer);
}

JNIEXPORT jbyteArray JNICALL
Java_app_openworld_core_Native_call(JNIEnv *env, jclass clazz, jbyteArray request, jobject sink) {
    char *text;
    char *answer;
    /* Events arrive on this thread before ow_call returns, so the local reference stays valid. */
    JobBox box = {NULL, sink};
    (void)clazz;
    text = to_cstr(env, request);
    if (text == NULL) {
        return NULL;
    }
    answer = ow_call(text, sink != NULL ? on_event : NULL, &box);
    free(text);
    return take_answer(env, answer);
}

JNIEXPORT jlong JNICALL
Java_app_openworld_core_Native_jobStart(JNIEnv *env, jclass clazz, jbyteArray request, jobject sink,
                                        jobjectArray refusal_out) {
    char *text;
    char *refusal = NULL;
    JobBox *box;
    ow_job *job;
    (void)clazz;
    text = to_cstr(env, request);
    if (text == NULL) {
        return 0;
    }
    box = calloc(1, sizeof *box);
    if (box == NULL) {
        free(text);
        return 0;
    }
    if (sink != NULL) {
        box->sink = (*env)->NewGlobalRef(env, sink);
    }
    job = ow_job_start(text, on_event, box, &refusal);
    free(text);
    if (job == NULL) {
        if (refusal_out != NULL && (*env)->GetArrayLength(env, refusal_out) > 0) {
            jbyteArray bytes = to_bytes(env, refusal);
            if (bytes != NULL) {
                (*env)->SetObjectArrayElement(env, refusal_out, 0, bytes);
                (*env)->DeleteLocalRef(env, bytes);
            }
        }
        ow_string_free(refusal);
        release_box(env, box);
        return 0;
    }
    box->job = job;
    return (jlong)(intptr_t)box;
}

JNIEXPORT jboolean JNICALL
Java_app_openworld_core_Native_jobWants(JNIEnv *env, jclass clazz, jlong handle, jlong index) {
    JobBox *box = box_of(handle);
    (void)env;
    (void)clazz;
    if (box == NULL || index < 0) {
        return JNI_FALSE;
    }
    return ow_job_wants(box->job, (uint64_t)index) ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT jboolean JNICALL
Java_app_openworld_core_Native_jobSkip(JNIEnv *env, jclass clazz, jlong handle, jlong index) {
    JobBox *box = box_of(handle);
    (void)env;
    (void)clazz;
    if (box == NULL || index < 0) {
        return JNI_FALSE;
    }
    return ow_job_skip(box->job, (uint64_t)index) ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT jboolean JNICALL
Java_app_openworld_core_Native_jobPushYuv(JNIEnv *env, jclass clazz, jlong handle, jlong index, jobject y,
                                          jint y_stride, jobject u, jobject v, jint uv_stride,
                                          jint uv_pixel_stride, jint width, jint height, jboolean full_range,
                                          jboolean bt709, jint rotation) {
    JobBox *box = box_of(handle);
    uint8_t *yp;
    uint8_t *up;
    uint8_t *vp;
    jlong y_cap;
    jlong u_cap;
    jlong v_cap;
    uint64_t cw;
    uint64_t ch;
    uint64_t y_need;
    uint64_t uv_need;
    (void)clazz;
    if (box == NULL || index < 0 || width <= 0 || height <= 0 || y_stride < width || uv_stride <= 0 ||
        uv_pixel_stride <= 0 || rotation < 0) {
        return JNI_FALSE;
    }
    y_cap = capacity(env, y, &yp);
    u_cap = capacity(env, u, &up);
    v_cap = capacity(env, v, &vp);
    if (y_cap < 0 || u_cap < 0 || v_cap < 0) {
        return JNI_FALSE;
    }
    /* The library reads exactly these many bytes from each plane. */
    cw = ((uint64_t)width + 1) / 2;
    ch = ((uint64_t)height + 1) / 2;
    y_need = (uint64_t)y_stride * (uint64_t)(height - 1) + (uint64_t)width;
    uv_need = (uint64_t)uv_stride * (ch - 1) + (cw - 1) * (uint64_t)uv_pixel_stride + 1;
    if ((uint64_t)y_cap < y_need || (uint64_t)u_cap < uv_need || (uint64_t)v_cap < uv_need) {
        return JNI_FALSE;
    }
    return ow_job_push_yuv420(box->job, (uint64_t)index, yp, (uint32_t)y_stride, up, vp, (uint32_t)uv_stride,
                              (uint32_t)uv_pixel_stride, (uint32_t)width, (uint32_t)height, full_range ? 1 : 0,
                              bt709 ? 1 : 0, (uint32_t)rotation)
               ? JNI_TRUE
               : JNI_FALSE;
}

JNIEXPORT jboolean JNICALL
Java_app_openworld_core_Native_jobPushPixels(JNIEnv *env, jclass clazz, jlong handle, jlong index, jobject pixels,
                                             jint width, jint height, jint stride, jint layout, jint rotation) {
    JobBox *box = box_of(handle);
    uint8_t *address;
    jlong cap;
    uint64_t bpp;
    (void)clazz;
    if (box == NULL || index < 0 || width <= 0 || height <= 0 || stride <= 0 || layout < 0 || layout > 2 ||
        rotation < 0) {
        return JNI_FALSE;
    }
    bpp = layout == 2 ? 3 : 4;
    cap = capacity(env, pixels, &address);
    if (cap < 0 || (uint64_t)stride < (uint64_t)width * bpp || (uint64_t)cap < (uint64_t)stride * (uint64_t)height) {
        return JNI_FALSE;
    }
    return ow_job_push_pixels(box->job, (uint64_t)index, address, (uint32_t)width, (uint32_t)height,
                              (uint32_t)stride, (uint32_t)layout, (uint32_t)rotation)
               ? JNI_TRUE
               : JNI_FALSE;
}

JNIEXPORT jlong JNICALL
Java_app_openworld_core_Native_jobStopToken(JNIEnv *env, jclass clazz, jlong handle) {
    JobBox *box = box_of(handle);
    ow_stop_token *token;
    (void)env;
    (void)clazz;
    if (box == NULL) {
        return 0;
    }
    token = ow_job_stop_token(box->job);
    return (jlong)(intptr_t)token;
}

JNIEXPORT void JNICALL
Java_app_openworld_core_Native_jobStop(JNIEnv *env, jclass clazz, jlong token, jbyteArray reason) {
    char *text;
    (void)clazz;
    if (token == 0) {
        return;
    }
    text = to_cstr(env, reason);
    if (text == NULL) {
        return;
    }
    ow_stop((ow_stop_token *)(intptr_t)token, text);
    free(text);
}

JNIEXPORT void JNICALL
Java_app_openworld_core_Native_jobStopFree(JNIEnv *env, jclass clazz, jlong token) {
    (void)env;
    (void)clazz;
    if (token != 0) {
        ow_stop_free((ow_stop_token *)(intptr_t)token);
    }
}

JNIEXPORT jbyteArray JNICALL
Java_app_openworld_core_Native_jobFinish(JNIEnv *env, jclass clazz, jlong handle, jboolean clean) {
    JobBox *box = box_of(handle);
    char *answer;
    jbyteArray out;
    (void)clazz;
    if (box == NULL) {
        return NULL;
    }
    /* Track events can still arrive while the job closes, so the sink lives until after. */
    answer = ow_job_finish(box->job, clean ? "{\"clean\":true}" : "{\"clean\":false}");
    box->job = NULL;
    out = take_answer(env, answer);
    release_box(env, box);
    return out;
}

JNIEXPORT void JNICALL
Java_app_openworld_core_Native_jobFree(JNIEnv *env, jclass clazz, jlong handle) {
    JobBox *box = box_of(handle);
    (void)clazz;
    if (box == NULL) {
        return;
    }
    ow_job_free(box->job);
    box->job = NULL;
    release_box(env, box);
}
