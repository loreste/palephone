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
