# Geliştirme ve test

```bash
cargo test                                   # paket ayrıştırma, strateji, kurallar, nft betiği
cargo check --target x86_64-pc-windows-gnu   # Windows (C derleyicisi gerekmez)
```

## Motoru root'suz gerçek çekirdekte denemek

Kullanıcı + ağ ad alanında nftables, NFQUEUE ve ham soketler çalışır. `203.0.113.1:443`'te
dinleyen bir sunucuya ClientHello gönderilip baytların bozulmadan vardığı ve
`desynced` sayacının arttığı doğrulanır:

```bash
unshare -rn sh -c 'ip link set lo up; ip addr add 203.0.113.1/32 dev lo; ...'
```

Motor stdin'i boru olmalı (FIFO'yu okuma-yazma açmak EOF'u engeller, temizlik test edilemez).

## Arayüz ekran görüntüsü (sanal ekran)

```bash
(echo 'S {...AppStatus...}'; sleep 8) | env -u WAYLAND_DISPLAY XDG_CONFIG_HOME=/tmp/qd \
  QDPI_SHOT=/tmp/qd.png QDPI_TAB=2 dbus-run-session -- xvfb-run -a ./target/debug/quickdpi --window
```

`dbus-run-session` gerçek masaüstünde tepsi simgesi ve parola penceresi açılmasını önler.
