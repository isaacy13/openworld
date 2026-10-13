/* SPDX-License-Identifier: Apache-2.0 */
/* Store package: compile this and link libopenworld_core. The class loads libopenworld_jni. */
#include <jni.h>
#include <stdlib.h>

char *ow_command(const char *request);
char *ow_command_progress(const char *request, void (*progress)(const char *json_line, void *user), void *user);
void ow_string_free(char *ptr);

JNIEXPORT jstring JNICALL
Java_app_openworld_Core_nativeCommand(JNIEnv *env, jclass clazz, jstring request) {
    const char *in_chars;
    char *out;
    jstring result;
    (void)clazz;
    if (request == NULL) {
        return (*env)->NewStringUTF(env, "{\"status\":\"refused\",\"message\":\"The scan request was empty. Refusing.\"}");
    }
    in_chars = (*env)->GetStringUTFChars(env, request, NULL);
    out = ow_command(in_chars);
    (*env)->ReleaseStringUTFChars(env, request, in_chars);
    result = (*env)->NewStringUTF(env, out != NULL ? out : "");
    ow_string_free(out);
    return result;
}

typedef struct {
    JNIEnv *env;
    jobject listener;
    jmethodID on_line;
} ProgressBox;

static void on_progress(const char *json_line, void *user) {
    ProgressBox *box = user;
    JNIEnv *env;
    jstring line;
    if (box == NULL || json_line == NULL || box->on_line == NULL) {
        return;
    }
    env = box->env;
    line = (*env)->NewStringUTF(env, json_line);
    if (line == NULL) {
        (*env)->ExceptionClear(env);
        return;
    }
    (*env)->CallVoidMethod(env, box->listener, box->on_line, line);
    (*env)->DeleteLocalRef(env, line);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
    }
}

JNIEXPORT jstring JNICALL
Java_app_openworld_Core_nativeCommandProgress(JNIEnv *env, jclass clazz, jstring request, jobject listener) {
    const char *in_chars;
    char *out;
    jstring result;
    jclass listener_class;
    ProgressBox box;
    (void)clazz;
    if (request == NULL) {
        return (*env)->NewStringUTF(env, "{\"status\":\"refused\",\"message\":\"The scan request was empty. Refusing.\"}");
    }
    if (listener == NULL) {
        return Java_app_openworld_Core_nativeCommand(env, clazz, request);
    }
    listener_class = (*env)->GetObjectClass(env, listener);
    box.env = env;
    box.listener = listener;
    box.on_line = (*env)->GetMethodID(env, listener_class, "onLine", "(Ljava/lang/String;)V");
    (*env)->DeleteLocalRef(env, listener_class);
    if (box.on_line == NULL) {
        (*env)->ExceptionClear(env);
        return (*env)->NewStringUTF(env, "{\"status\":\"refused\",\"message\":\"The scan progress callback is missing. Refusing.\"}");
    }
    in_chars = (*env)->GetStringUTFChars(env, request, NULL);
    out = ow_command_progress(in_chars, on_progress, &box);
    (*env)->ReleaseStringUTFChars(env, request, in_chars);
    result = (*env)->NewStringUTF(env, out != NULL ? out : "");
    ow_string_free(out);
    return result;
}
