# The JNI shim finds these by name.
-keep interface app.openworld.core.EventSink { *; }
-keepclassmembers class * implements app.openworld.core.EventSink {
    public void onEvent(byte[]);
}
-keepclasseswithmembernames class app.openworld.core.Native {
    native <methods>;
}
