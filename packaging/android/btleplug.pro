# R8 rules for btleplug's Android backend (droidplug). Its Java classes are only ever reached
# through JNI (class and method lookups by name from Rust), so the shrinker sees no caller and
# would remove or rename them: "failed to resolve java class ... class not found".
-keep class com.nonpolynomial.btleplug.** { *; }
-keep class io.github.gedgygedgy.rust.** { *; }
