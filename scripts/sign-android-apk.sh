#!/usr/bin/env bash
# Sign an Android APK for sideload installs (v2/v3).
#
# Usage:
#   ./scripts/sign-android-apk.sh path/to/app-unsigned.apk [output.apk]
#
# Default keystore: packaging/android/pale-sideload.jks (public sideload cert).
# Override with:
#   PALE_ANDROID_KEYSTORE=/path/to.jks
#   PALE_ANDROID_KEY_ALIAS=pale
#   PALE_ANDROID_KEYSTORE_PASSWORD=...
#   PALE_ANDROID_KEY_PASSWORD=...
#
# Requires Java 17+ and network once to fetch uber-apk-signer (or set
# UBER_APK_SIGNER_JAR to a local jar).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IN_APK="${1:-}"
OUT_APK="${2:-}"

if [ -z "$IN_APK" ] || [ ! -f "$IN_APK" ]; then
  echo "usage: $0 <unsigned.apk> [signed.apk]" >&2
  exit 1
fi

KEYSTORE="${PALE_ANDROID_KEYSTORE:-$ROOT/packaging/android/pale-sideload.jks}"
ALIAS="${PALE_ANDROID_KEY_ALIAS:-pale}"
STORE_PASS="${PALE_ANDROID_KEYSTORE_PASSWORD:-palesideload}"
KEY_PASS="${PALE_ANDROID_KEY_PASSWORD:-$STORE_PASS}"

if [ ! -f "$KEYSTORE" ]; then
  echo "error: keystore not found: $KEYSTORE" >&2
  exit 1
fi

if ! command -v java >/dev/null 2>&1; then
  echo "error: java not found (need JDK 17+)" >&2
  exit 1
fi

SIGNER_JAR="${UBER_APK_SIGNER_JAR:-}"
if [ -z "$SIGNER_JAR" ] || [ ! -f "$SIGNER_JAR" ]; then
  CACHE_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/pale"
  mkdir -p "$CACHE_DIR"
  SIGNER_JAR="$CACHE_DIR/uber-apk-signer-1.3.0.jar"
  if [ ! -f "$SIGNER_JAR" ]; then
    echo "Downloading uber-apk-signer 1.3.0..."
    curl -fsSL -o "$SIGNER_JAR" \
      "https://github.com/patrickfav/uber-apk-signer/releases/download/v1.3.0/uber-apk-signer-1.3.0.jar"
  fi
fi

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT
cp "$IN_APK" "$WORKDIR/in.apk"

java -jar "$SIGNER_JAR" \
  --apks "$WORKDIR/in.apk" \
  --out "$WORKDIR/out" \
  --ks "$KEYSTORE" \
  --ksAlias "$ALIAS" \
  --ksPass "$STORE_PASS" \
  --ksKeyPass "$KEY_PASS" \
  --allowResign

SIGNED="$(find "$WORKDIR/out" -name '*-aligned-signed.apk' -o -name '*-signed.apk' | head -1)"
if [ -z "$SIGNED" ] || [ ! -f "$SIGNED" ]; then
  echo "error: signed APK not produced" >&2
  ls -la "$WORKDIR/out" >&2 || true
  exit 1
fi

if [ -z "$OUT_APK" ]; then
  base="$(basename "$IN_APK")"
  base="${base%.apk}"
  base="${base%-unsigned}"
  OUT_APK="$(dirname "$IN_APK")/${base}-signed.apk"
fi

mkdir -p "$(dirname "$OUT_APK")"
cp "$SIGNED" "$OUT_APK"
echo "Signed APK: $OUT_APK"
shasum -a 256 "$OUT_APK" | tee "${OUT_APK}.sha256"
