# Mimari

## Süreçler

| Süreç | Komut | Yetki | Görev |
|---|---|---|---|
| Tepsi / denetleyici | `quickdpi [--hidden]` | kullanıcı (Windows: yönetici) | Ayar, motor yönetimi, otomatik ayar, tepsi simgesi |
| Pencere | `quickdpi --window` | tepsiyle aynı | egui arayüzü; kapanınca süreç biter, tepsi sürer |
| Motor (yalnız Linux) | `pkexec quickdpi --engine` | root | nftables + NFQUEUE + ham soket |

- Tepsi ↔ pencere: satır protokolü (`src/proto.rs`). Tepsi → pencere `S <AppStatus>`, `show`, `quit`;
  pencere → tepsi `Cmd` JSON. Pencere ayarı dosyaya yazar, `Reload` gönderir.
- Tepsi ↔ motor (Linux): stdin'e `EngineConfig` JSON satırları, stdout'tan `EngineStatus` JSON.
  stdin kapanınca motor kuralları siler ve çıkar (uygulama çökse de iz kalmaz; kuyruk `bypass`
  bayraklı olduğundan motor yokken trafik akmaya devam eder).
- Tek örnek: `~/.config/quickdpi/instance.port` üzerinden yerel TCP; ikinci açılış pencereyi gösterir.
- Windows'ta motor tepsi sürecinin içindedir (`src/engine/windows.rs`); WinDivert DLL ve sürücüsü
  `%ProgramData%\QuickDPI\WinDivert` altına çıkarılıp dinamik yüklenir.

## Paket kararı (`src/engine/mod.rs`)

1. Yalnızca 80/443'e giden ilk veri paketleri gelir (Linux: `ct original packets 1-6`).
2. ClientHello'dan SNI, HTTP'den Host okunur; istisna kuralı tutarsa paket aynen geçer.
3. Strateji: sahte paket(ler) → bölünmüş parçalar. Son parça kuyruktaki paketin yerine geçer,
   öncekiler ham soketle (işaretli, `meta mark 0x4451`) gönderilir.
4. Otomatik TTL: gelen SYN-ACK'in TTL'sinden sunucuya sekme sayısı çıkarılır.

## Hazır ayarlar (`src/config.rs`)

Hafiften ağıra: Bölme (2. bayt) → SNI'dan ters bölme → sahte (bozuk sağlama) → sahte (otomatik TTL)
→ sahte (yanlış sıra + bozuk sağlama) → hepsi. Türk Telekom hattında (Kasım 2026) **2. bayttan
bölme + DNS yönlendirme** yeterli; SNI ortasından bölme yetmiyor.
