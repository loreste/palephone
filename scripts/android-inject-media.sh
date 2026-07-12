#!/usr/bin/env bash
# Inject PJSIP camera Java + Pale video Kotlin into a generated Tauri Android project.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
GEN="$ROOT/src-tauri/gen/android"
if [ ! -d "$GEN" ]; then
  echo "error: run 'npm run tauri android init' first (missing $GEN)" >&2
  exit 1
fi

JAVA_DIR="$GEN/app/src/main/java/com/pale/softphone"
mkdir -p "$JAVA_DIR"
cp "$ROOT/src-tauri/android/"*.kt "$JAVA_DIR/"

PJSIP_JAVA="$GEN/app/src/main/java/org/pjsip"
mkdir -p "$PJSIP_JAVA"
cp "$ROOT/packaging/android/java/org/pjsip/"*.java "$PJSIP_JAVA/"
echo "Copied $(ls "$PJSIP_JAVA" | wc -l | tr -d ' ') PJSIP Java sources to $PJSIP_JAVA"

# R8/minify strips classes only referenced from native code unless kept.
PROGUARD_DIR="$GEN/app"
for rules in \
  "$PROGUARD_DIR/proguard-rules.pro" \
  "$PROGUARD_DIR/src/main/proguard-rules.pro"
do
  if [ -f "$rules" ] || [ -d "$(dirname "$rules")" ]; then
    mkdir -p "$(dirname "$rules")"
    if [ ! -f "$rules" ]; then
      touch "$rules"
    fi
    if ! grep -q 'org.pjsip' "$rules" 2>/dev/null; then
      cat >> "$rules" <<'KEEP'
# Pale / PJSIP: camera classes are only reached via JNI from libpale_lib.so
-keep class org.pjsip.** { *; }
-keepclassmembers class org.pjsip.** { *; }
-keep class com.pale.softphone.PaleJni { *; }
-keep class com.pale.softphone.PaleVideoOverlay { *; }
KEEP
      echo "Added ProGuard keep rules to $rules"
    fi
  fi
done

# Also force a Kotlin reference so the package is on the classpath even without minify.
# (MainActivity will call PaleJni; add a no-op touch of PjCameraInfo2 via PaleJni.prepare)
if ! grep -q 'org.pjsip' "$JAVA_DIR/PaleJni.kt" 2>/dev/null; then
  # Ensure compiler sees the Java package (prevents empty package elimination).
  sed -i.bak 's/fun prepare(activity: Activity) {/fun prepare(activity: Activity) {\n        \/\/ Touch PJSIP camera package so R8 cannot strip org.pjsip.*\n        try { Class.forName("org.pjsip.PjCamera2") } catch (_: Throwable) {}/' \
    "$JAVA_DIR/PaleJni.kt" || true
fi

MAIN=$(find "$GEN" \( -name 'MainActivity.kt' -o -name 'MainActivity.java' \) | head -1)
if [ -n "$MAIN" ] && ! grep -q 'PaleJni.prepare' "$MAIN"; then
  if grep -q 'super.onCreate' "$MAIN"; then
    if [[ "$MAIN" == *.kt ]]; then
      sed -i.bak '/super.onCreate/a\    com.pale.softphone.PaleJni.prepare(this)' "$MAIN"
    else
      sed -i.bak '/super.onCreate/a\    com.pale.softphone.PaleJni.prepare(this);' "$MAIN"
    fi
    echo "Hooked PaleJni.prepare into $MAIN"
  else
    echo "warn: could not find super.onCreate in $MAIN — call PaleJni.prepare(this) manually" >&2
  fi
fi

echo "Android media inject complete."
