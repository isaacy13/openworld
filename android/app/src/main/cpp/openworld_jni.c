/* SPDX-License-Identifier: Apache-2.0 */
/* Store package: compile this and link libopenworld_core. The class loads libopenworld_jni. */
#include <jni.h>
#include <stdlib.h>

char *ow_command(const char *request);
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
