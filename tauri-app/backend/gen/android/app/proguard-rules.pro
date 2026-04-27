# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# VaultStorage is invoked from Rust via JNI lookup-by-name. R8 sees no
# Kotlin/Java callers and would otherwise rename or strip it, causing JNI
# class lookup to fail at runtime and the Rust keychain layer to silently
# fall back to in-memory storage (vault wipes on every app restart).
-keep class com.nostr.mail.VaultStorage { *; }
-keep class com.nostr.mail.VaultStorage$* { *; }

# Jetpack Security (EncryptedFile/MasterKey) is implemented on Tink, which
# relies on reflection for keyset and primitive registration. Without these
# rules, EncryptedFile decryption fails in release builds.
-keep class androidx.security.crypto.** { *; }
-keep class com.google.crypto.tink.** { *; }
-keepclassmembers class com.google.crypto.tink.** { *; }
-keep class com.google.crypto.tink.proto.** { *; }
-dontwarn com.google.crypto.tink.**