# Android

Saf Kotlin, yalnız Android çerçevesi (AndroidX yok, yerel kitaplık yok); `android/` altında ayrı Gradle
projesi. minSdk 29 (Android 10), targetSdk 36. Sürüm kökteki `Cargo.toml`'dan okunur
(`1.2.3` → versionCode `10203`).

## Çalışma şekli

- `QuickVpnService` (VpnService): tünele yalnız sanal DNS adresleri girer (`10.111.222.1/32`,
  `fd00:111:222::1/128`). Tünelden gelen UDP/53 sorguları (`Packets.kt`) seçilen sunucuya iletilir
  (`Dns.kt`), cevap paketi tünele yazılır.
- Diğer trafik tünele girmez: VPN, `setHttpProxy` ile sistem proxy'si olarak yerel proxy'yi
  (`Proxy.kt`, `127.0.0.1:<rastgele>`) duyurur. Tarayıcı ve uygulamalar CONNECT ile gelir; sunucuya
  giden ilk veri (ClientHello) stratejiye göre bölünür (TCP_NODELAY + ayrı yazmalar) ya da iki TLS
  kaydına ayrılır. "Hariç tut" kipindeki siteler proxy istisna listesine yazılır, hiç uğramaz.
- Uygulama kendi soketleri döngüye girmesin diye `addDisallowedApplication(packageName)` ile VPN dışında.
- Otomatik ayar (`AutoTune.kt`): her strateji için geçici bir yerel proxy açar, test sitelerini
  onun üzerinden ister; tümünü açan ilk (hafiften ağıra) strateji seçilir.
- Kullanım koşulları kabul edilmeden (`Prefs.acceptedTerms` < `Texts.TERMS_VERSION`) hizmet, kutucuk ve
  açılış alıcısı korumayı başlatmaz.
- Proxy kullanmayan uygulamalar (kendi ağ yığını olanlar) yalnız güvenli DNS'ten yararlanır.

## Derleme

```bash
cd android
JAVA_HOME=~/.jdks/jdk-21.0.13+9 ./gradlew assembleRelease
# → app/build/outputs/apk/release/app-release.apk
```

`local.properties` (sdk.dir) repoda yok; Android Studio ya da `ANDROID_HOME` sağlar.

Yayın imzası ortam değişkenlerinden: `QUICKDPI_KEYSTORE` (dosya yolu), `QUICKDPI_KEYSTORE_PASS`,
`QUICKDPI_KEY_ALIAS` (varsayılan `quickdpi`), `QUICKDPI_KEY_PASS`. CI'da `ci/build-android.sh`
`QUICKDPI_KEYSTORE_B64`'ü açar. Yoksa geliştirme anahtarıyla imzalanır (kurulur, ama yayın APK'sının
üstüne güncelleme olarak kurulamaz). Anahtar bir kez üretilir, kaybedilmez:

```bash
keytool -genkeypair -v -keystore quickdpi-release.jks -alias quickdpi -keyalg RSA -keysize 4096 -validity 36500
base64 -w0 quickdpi-release.jks   # → QUICKDPI_KEYSTORE_B64
```

## Test (emülatör, kullanıcıyı rahatsız etmeden)

```bash
avdmanager create avd -n QuickDPI_Test -k "system-images;android-36;google_apis;x86_64" -d pixel_6
emulator -avd QuickDPI_Test -port 5640 -no-window -no-audio -no-snapshot -gpu swiftshader_indirect &
adb -s emulator-5640 install -r app/build/outputs/apk/release/app-release.apk
adb -s emulator-5640 shell appops set dev.earthquick.quickdpi ACTIVATE_VPN allow   # VPN onay penceresi yok
adb -s emulator-5640 shell am start -n dev.earthquick.quickdpi/.MainActivity --ez start true
```

Emülatörün trafiği bilgisayardan çıkar: masaüstü QuickDPI açıksa onu da korur, test geçersiz olur —
önce masaüstündekini kapatın. Ekranı `uiautomator dump` ile okuyun, Chrome ile site açıp
`screencap` alın. Kasım 2026'da Türk Telekom hattında "Bölme (2. bayt)" 3/3 seçildi; discord ve
wattpad Chrome'da açıldı.
