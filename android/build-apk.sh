#!/usr/bin/env bash
# Gera o APK Android (arm64) sem Gradle: compila a biblioteca Rust com o NDK e
# monta o pacote com as ferramentas do SDK (aapt2, zipalign, apksigner).
#
# Precisa de: rustup target add aarch64-linux-android, ANDROID_NDK_HOME,
# ANDROID_HOME (com build-tools e uma platform) e Java (keytool/apksigner).
#
# Assinatura: usa $KEYSTORE / $KEYSTORE_PASS se existirem; senão cria uma
# chave de depuração em target/android/debug.keystore.
#
# Uso: android/build-apk.sh [saida.apk]
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-target/android/MonitorRede-android-arm64.apk}"
MIN_SDK=26
TARGET_SDK=34
TRIPLE=aarch64-linux-android

die() { echo "erro: $*" >&2; exit 1; }

NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
[ -d "$NDK" ] || die "defina ANDROID_NDK_HOME (pasta do NDK)"
[ -d "$SDK" ] || die "defina ANDROID_HOME (pasta do SDK)"

case "$(uname -s)" in
  Darwin) HOST=darwin-x86_64 ;;
  Linux) HOST=linux-x86_64 ;;
  *) die "sistema não suportado: $(uname -s)" ;;
esac
TC="$NDK/toolchains/llvm/prebuilt/$HOST/bin"
[ -x "$TC/${TRIPLE}${MIN_SDK}-clang" ] || die "clang do NDK não encontrado em $TC"

# --- biblioteca nativa
export CC_aarch64_linux_android="$TC/${TRIPLE}${MIN_SDK}-clang"
export AR_aarch64_linux_android="$TC/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
# Páginas de 16 KB (exigidas em aparelhos novos com Android 15+).
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
cargo rustc --release --locked --lib --target "$TRIPLE" --crate-type cdylib
SO="target/$TRIPLE/release/libmonitor_rede.so"
[ -f "$SO" ] || die "$SO não foi gerado"

# --- ferramentas do SDK (versão mais nova instalada)
BT="$SDK/build-tools/$(ls "$SDK/build-tools" | sort -V | tail -1)"
JAR=""
for p in $(ls -d "$SDK"/platforms/android-* | sort -V -r); do
  if [ -f "$p/android.jar" ]; then JAR="$p/android.jar"; break; fi
done
[ -x "$BT/aapt2" ] || die "build-tools não encontrado em $SDK/build-tools"
[ -n "$JAR" ] || die "nenhuma platform com android.jar em $SDK/platforms"
echo "build-tools: $BT"
echo "android.jar: $JAR"

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
IFS=. read -r MAJOR MINOR PATCH <<< "$VERSION"
CODE=$((MAJOR * 10000 + MINOR * 100 + PATCH))

# --- pacote
W=target/android/apk
rm -rf "$W"
mkdir -p "$W/lib/arm64-v8a" "$(dirname "$OUT")"
"$BT/aapt2" compile --dir android/res -o "$W/res.zip"
"$BT/aapt2" link -o "$W/unsigned.apk" -I "$JAR" \
  --manifest android/AndroidManifest.xml \
  --min-sdk-version "$MIN_SDK" --target-sdk-version "$TARGET_SDK" \
  --version-code "$CODE" --version-name "$VERSION" \
  "$W/res.zip"
cp "$SO" "$W/lib/arm64-v8a/"
(cd "$W" && zip -q -r unsigned.apk lib)
"$BT/zipalign" -f 4 "$W/unsigned.apk" "$W/aligned.apk"

# --- assinatura
KS="${KEYSTORE:-}"
PASS="${KEYSTORE_PASS:-android}"
if [ -z "$KS" ]; then
  KS=target/android/debug.keystore
  if [ ! -f "$KS" ]; then
    keytool -genkeypair -keystore "$KS" -storepass "$PASS" -keypass "$PASS" \
      -alias monitorrede -keyalg RSA -keysize 2048 -validity 10000 \
      -dname "CN=Monitor de Rede, O=hfabio, C=BR"
  fi
fi
"$BT/apksigner" sign --v4-signing-enabled false --ks "$KS" --ks-pass "pass:$PASS" --key-pass "pass:$PASS" --out "$OUT" "$W/aligned.apk"
"$BT/apksigner" verify "$OUT"
echo "APK: $OUT ($(du -h "$OUT" | cut -f1))"
