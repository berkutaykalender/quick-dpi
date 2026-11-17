<p align="center"><img src="assets/quickdpi.svg" width="96" alt="QuickDPI"></p>

# QuickDPI

**Güvenmediğiniz ağlar için ek koruma katmanı.** Ortak Wi-Fi'larda, otel ve kafe ağlarında ya da
ele geçirilmiş bir modemin arkasında trafiğinizi izlemeye ve sahte DNS cevaplarıyla sizi taklit
sitelere yönlendirmeye çalışanlara karşı cihazınızda çalışan, açık kaynaklı bir ağ güvenliği aracı.
Windows, Linux ve Android. Ek program gerekmez, ayarını kendisi yapar.

Site ve indirmeler: **<https://quick-dpi.earthquick.dev>** · [Sürümler](https://github.com/berkutaykalender/quick-dpi/releases) · [English](#english)

## Ne yapar?

- **Güvenli DNS**: alan adı sorgularını seçtiğiniz güvenilir sunucuya (21 hazır seçenek ya da kendi
  sunucunuz) standart dışı bir porttan gönderir; ağdaki birinin sahte cevapla sizi yönlendirmesi zorlaşır.
- **Bağlantı gizliliği**: bağlantının site adını taşıyan ilk paketini (TLS ClientHello / HTTP isteği)
  parçalara bölerek gönderir; pasif izleme ve paket inceleme cihazlarının site adını okuması zorlaşır.
- **Kendini ayarlar**: ilk açılışta hazır ayarları hafiften ağıra dener, ağınıza uyanı seçer.
- **Site kuralları**: korumayı tüm sitelere ya da yalnız seçtiklerinize uygulayın.
- **Arka planda**: masaüstünde bildirim alanında, Android'de hızlı ayarlar kutucuğunda bekler.
- **Kendini günceller**: yeni sürümü siteden indirir, sağlamasını doğrulayıp kurar.

QuickDPI bir VPN **değildir**: trafiği şifrelemez, IP adresini gizlemez, hiçbir sunucuya taşımaz.
Kişisel veri toplamaz.

## Kullanım

| Platform | Dosya | Not |
|---|---|---|
| Windows 10/11 | `quickdpi-windows-x86_64.exe` | Tek dosya. Yönetici izni ister; WinDivert sürücüsü exe'nin içinde. |
| Linux | `quickdpi_amd64.deb` ya da `quickdpi-linux-x86_64.tar.gz` | `nft` ve `pkexec` gerekir (dağıtımlarda hazır). Deb ile kurulunca parola sorulmaz. |
| Android 10+ | `quickdpi-android.apk` | Root gerekmez. Yalnız DNS için yerel VPN arayüzü kullanır. |

İlk açılışta kullanım koşulları gösterilir; kabul edilmeden koruma başlamaz.

Masaüstü kısayolları: `Ctrl+E` aç/kapat · `Ctrl+T` otomatik ayar · `Ctrl+L` kural ekle ·
`Ctrl+1..5` sekmeler · `Esc`/`Ctrl+W` tepsiye · `Ctrl+Q` çıkış.

## Sorumluluk

Yazılım "olduğu gibi", hiçbir garanti verilmeden sunulur. Yalnızca güvenmediğiniz ağlarda
kullanılmak üzere tasarlanmıştır; bir kurumun ağında kullanmadan önce ağ yöneticisinin iznini alın.
Kullanımından doğan tüm sorumluluk kullanıcıya aittir; bulunduğunuz yerin yasalarına uymak sizin
sorumluluğunuzdadır.

## Geliştirme

```bash
cargo run --release                      # masaüstü (Linux/Windows)
cd android && ./gradlew assembleRelease  # Android (JDK 17+, Android SDK)
```

Belgeler: [mimari](docs/mimari.md) · [geliştirme ve test](docs/gelistirme.md) ·
[Android](docs/android.md).

## Lisans

[MIT](LICENSE). Windows sürümüne gömülü WinDivert 2.2.2 LGPLv3 lisanslıdır (`vendor/windivert/LICENSE`).

---

## English

**An extra layer of protection for networks you don't trust.** QuickDPI is an open-source network
security tool for Windows, Linux and Android. On public Wi-Fi, hotel and café networks or behind a
compromised router it protects you against traffic monitoring and forged DNS answers:

- **Secure DNS** — lookups go to a trusted server of your choice on a non-standard port.
- **Connection privacy** — the first packet of a connection, which carries the site name, is sent in
  pieces so passive monitoring and packet-inspection devices can't easily read it.
- **Configures itself**, **site rules**, **updates itself**, runs quietly in the tray / a Quick Settings tile.

It is not a VPN: it does not encrypt your traffic or hide your IP, and it collects no data.
Provided "as is"; you are responsible for how you use it. Downloads: <https://quick-dpi.earthquick.dev>.
