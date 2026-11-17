use std::net::IpAddr;

pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

#[derive(Clone, Copy, Debug)]
pub struct Ip {
    pub v6: bool,
    pub ihl: usize,
    pub proto: u8,
    pub l4: usize,
    pub l4len: usize,
    pub total: usize,
}

impl Ip {
    pub fn payload(&self) -> usize {
        self.l4 + self.l4len
    }
}

pub fn parse(p: &[u8]) -> Option<Ip> {
    let ver = p.first()? >> 4;
    let (v6, ihl, proto, total) = match ver {
        4 => {
            if p.len() < 20 {
                return None;
            }
            let ihl = ((p[0] & 0x0f) as usize) * 4;
            let frag = u16::from_be_bytes([p[6], p[7]]);
            if frag & 0x3fff != 0 {
                return None;
            }
            (false, ihl, p[9], u16::from_be_bytes([p[2], p[3]]) as usize)
        }
        6 => {
            if p.len() < 40 {
                return None;
            }
            (true, 40, p[6], 40 + u16::from_be_bytes([p[4], p[5]]) as usize)
        }
        _ => return None,
    };
    if ihl < 20 || total > p.len() || total < ihl {
        return None;
    }
    let l4len = match proto {
        PROTO_TCP => {
            if total < ihl + 20 {
                return None;
            }
            ((p[ihl + 12] >> 4) as usize) * 4
        }
        PROTO_UDP => 8,
        _ => return None,
    };
    if l4len < 8 || ihl + l4len > total {
        return None;
    }
    Some(Ip { v6, ihl, proto, l4: ihl, l4len, total })
}

pub fn src_ip(p: &[u8], ip: &Ip) -> IpAddr {
    if ip.v6 {
        IpAddr::from(<[u8; 16]>::try_from(&p[8..24]).unwrap())
    } else {
        IpAddr::from(<[u8; 4]>::try_from(&p[12..16]).unwrap())
    }
}

pub fn dst_ip(p: &[u8], ip: &Ip) -> IpAddr {
    if ip.v6 {
        IpAddr::from(<[u8; 16]>::try_from(&p[24..40]).unwrap())
    } else {
        IpAddr::from(<[u8; 4]>::try_from(&p[16..20]).unwrap())
    }
}

pub fn set_src_ip(p: &mut [u8], ip: &Ip, a: IpAddr) {
    match a {
        IpAddr::V4(a) if !ip.v6 => p[12..16].copy_from_slice(&a.octets()),
        IpAddr::V6(a) if ip.v6 => p[8..24].copy_from_slice(&a.octets()),
        _ => {}
    }
}

pub fn set_dst_ip(p: &mut [u8], ip: &Ip, a: IpAddr) {
    match a {
        IpAddr::V4(a) if !ip.v6 => p[16..20].copy_from_slice(&a.octets()),
        IpAddr::V6(a) if ip.v6 => p[24..40].copy_from_slice(&a.octets()),
        _ => {}
    }
}

pub fn src_port(p: &[u8], ip: &Ip) -> u16 {
    u16::from_be_bytes([p[ip.l4], p[ip.l4 + 1]])
}

pub fn dst_port(p: &[u8], ip: &Ip) -> u16 {
    u16::from_be_bytes([p[ip.l4 + 2], p[ip.l4 + 3]])
}

pub fn set_src_port(p: &mut [u8], ip: &Ip, v: u16) {
    p[ip.l4..ip.l4 + 2].copy_from_slice(&v.to_be_bytes());
}

pub fn set_dst_port(p: &mut [u8], ip: &Ip, v: u16) {
    p[ip.l4 + 2..ip.l4 + 4].copy_from_slice(&v.to_be_bytes());
}

pub fn ttl(p: &[u8], ip: &Ip) -> u8 {
    if ip.v6 { p[7] } else { p[8] }
}

pub fn set_ttl(p: &mut [u8], ip: &Ip, v: u8) {
    if ip.v6 {
        p[7] = v
    } else {
        p[8] = v
    }
}

pub fn tcp_seq(p: &[u8], ip: &Ip) -> u32 {
    u32::from_be_bytes(p[ip.l4 + 4..ip.l4 + 8].try_into().unwrap())
}

pub fn set_tcp_seq(p: &mut [u8], ip: &Ip, v: u32) {
    p[ip.l4 + 4..ip.l4 + 8].copy_from_slice(&v.to_be_bytes());
}

pub fn tcp_flags(p: &[u8], ip: &Ip) -> u8 {
    p[ip.l4 + 13]
}

pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
#[allow(dead_code)]
pub const TCP_PSH: u8 = 0x08;
pub const TCP_ACK: u8 = 0x10;

fn sum16(data: &[u8], mut acc: u32) -> u32 {
    let mut chunks = data.chunks_exact(2);
    for c in &mut chunks {
        acc += u16::from_be_bytes([c[0], c[1]]) as u32;
    }
    if let [b] = chunks.remainder() {
        acc += (*b as u32) << 8;
    }
    acc
}

fn fold(mut acc: u32) -> u16 {
    while acc >> 16 != 0 {
        acc = (acc & 0xffff) + (acc >> 16);
    }
    !(acc as u16)
}

pub fn fix(p: &mut [u8]) {
    let Some(ip) = parse_lenient(p) else { return };
    let len = p.len();
    if ip.v6 {
        p[4..6].copy_from_slice(&((len - 40) as u16).to_be_bytes());
    } else {
        p[2..4].copy_from_slice(&(len as u16).to_be_bytes());
        p[10] = 0;
        p[11] = 0;
        let c = fold(sum16(&p[..ip.ihl], 0));
        p[10..12].copy_from_slice(&c.to_be_bytes());
    }
    let l4len = len - ip.l4;
    let ck_off = match ip.proto {
        PROTO_TCP => ip.l4 + 16,
        PROTO_UDP => {
            p[ip.l4 + 4..ip.l4 + 6].copy_from_slice(&(l4len as u16).to_be_bytes());
            ip.l4 + 6
        }
        _ => return,
    };
    p[ck_off] = 0;
    p[ck_off + 1] = 0;
    let mut acc = if ip.v6 { sum16(&p[8..40], 0) } else { sum16(&p[12..20], 0) };
    acc += ip.proto as u32;
    acc += l4len as u32;
    let mut c = fold(sum16(&p[ip.l4..], acc));
    if ip.proto == PROTO_UDP && c == 0 {
        c = 0xffff;
    }
    p[ck_off..ck_off + 2].copy_from_slice(&c.to_be_bytes());
}

fn parse_lenient(p: &[u8]) -> Option<Ip> {
    let mut tmp = p.to_vec();
    let len = tmp.len();
    match tmp.first()? >> 4 {
        4 if len >= 20 => tmp[2..4].copy_from_slice(&(len as u16).to_be_bytes()),
        6 if len >= 40 => tmp[4..6].copy_from_slice(&((len - 40) as u16).to_be_bytes()),
        _ => return None,
    }
    parse(&tmp)
}

pub fn segment(p: &[u8], ip: &Ip, payload: &[u8], seq: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(ip.payload() + payload.len());
    out.extend_from_slice(&p[..ip.payload()]);
    out.extend_from_slice(payload);
    set_tcp_seq(&mut out, ip, seq);
    fix(&mut out);
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostAt {
    pub host: String,
    pub offset: usize,
}

pub fn is_client_hello(d: &[u8]) -> bool {
    d.len() > 5 && d[0] == 0x16 && d[1] == 0x03 && d[5] == 0x01
}

pub fn tls_sni(d: &[u8]) -> Option<HostAt> {
    if !is_client_hello(d) {
        return None;
    }
    let mut i = 5 + 4;
    i += 2 + 32;
    let sid = *d.get(i)? as usize;
    i += 1 + sid;
    let cs = u16::from_be_bytes([*d.get(i)?, *d.get(i + 1)?]) as usize;
    i += 2 + cs;
    let comp = *d.get(i)? as usize;
    i += 1 + comp;
    i += 2;
    while i + 4 <= d.len() {
        let typ = u16::from_be_bytes([d[i], d[i + 1]]);
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let body = i + 4;
        if typ == 0 {
            let name_len = u16::from_be_bytes([*d.get(body + 3)?, *d.get(body + 4)?]) as usize;
            let start = body + 5;
            let name = d.get(start..start + name_len)?;
            let host = std::str::from_utf8(name).ok()?.to_ascii_lowercase();
            return Some(HostAt { host, offset: start });
        }
        i = body + len;
    }
    None
}

const HTTP_METHODS: [&[u8]; 8] = [b"GET ", b"POST ", b"HEAD ", b"PUT ", b"OPTIONS ", b"DELETE ", b"PATCH ", b"CONNECT "];

pub fn is_http_request(d: &[u8]) -> bool {
    HTTP_METHODS.iter().any(|m| d.starts_with(m))
}

pub fn http_host(d: &[u8]) -> Option<HostAt> {
    if !is_http_request(d) {
        return None;
    }
    let pos = find_ci(d, b"\r\nhost:")?;
    let mut start = pos + 7;
    while d.get(start) == Some(&b' ') || d.get(start) == Some(&b'\t') {
        start += 1;
    }
    let end = start + d[start..].iter().position(|&b| b == b'\r' || b == b'\n')?;
    let raw = std::str::from_utf8(&d[start..end]).ok()?.trim();
    let host = if raw.starts_with('[') { raw.trim_start_matches('[').split(']').next()? } else { raw.split(':').next()? };
    Some(HostAt { host: host.to_ascii_lowercase(), offset: start })
}

pub fn http_mix_host(d: &mut [u8]) -> bool {
    if let Some(pos) = find_ci(d, b"\r\nhost:") {
        d[pos + 2..pos + 6].copy_from_slice(b"hoSt");
        return true;
    }
    false
}

fn find_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle))
}

pub fn fake_client_hello(sni: &str) -> Vec<u8> {
    let mut ext = Vec::new();
    let put_ext = |ext: &mut Vec<u8>, typ: u16, body: &[u8]| {
        ext.extend_from_slice(&typ.to_be_bytes());
        ext.extend_from_slice(&(body.len() as u16).to_be_bytes());
        ext.extend_from_slice(body);
    };
    let name = sni.as_bytes();
    let mut sn = Vec::new();
    sn.extend_from_slice(&((name.len() + 3) as u16).to_be_bytes());
    sn.push(0);
    sn.extend_from_slice(&(name.len() as u16).to_be_bytes());
    sn.extend_from_slice(name);
    put_ext(&mut ext, 0x0000, &sn);
    put_ext(&mut ext, 0x0017, &[]);
    put_ext(&mut ext, 0xff01, &[0]);
    put_ext(&mut ext, 0x000a, &[0, 6, 0x00, 0x1d, 0x00, 0x17, 0x00, 0x18]);
    put_ext(&mut ext, 0x000b, &[1, 0]);
    put_ext(&mut ext, 0x0010, &[0, 12, 2, b'h', b'2', 8, b'h', b't', b't', b'p', b'/', b'1', b'.', b'1']);
    put_ext(&mut ext, 0x000d, &[0, 8, 0x04, 0x03, 0x08, 0x04, 0x04, 0x01, 0x05, 0x03]);
    let mut ks = vec![0, 36, 0x00, 0x1d, 0, 32];
    ks.extend((0..32u8).map(|i| i.wrapping_mul(37).wrapping_add(11)));
    put_ext(&mut ext, 0x0033, &ks);
    put_ext(&mut ext, 0x002d, &[1, 1]);
    put_ext(&mut ext, 0x002b, &[4, 0x03, 0x04, 0x03, 0x03]);

    let mut hello = Vec::new();
    hello.extend_from_slice(&[0x03, 0x03]);
    hello.extend((0..32u8).map(|i| i.wrapping_mul(73).wrapping_add(5)));
    hello.push(32);
    hello.extend((0..32u8).map(|i| i.wrapping_mul(29).wrapping_add(3)));
    let suites: [u16; 9] = [0x1301, 0x1302, 0x1303, 0xc02b, 0xc02f, 0xc02c, 0xc030, 0xcca9, 0xcca8];
    hello.extend_from_slice(&((suites.len() * 2) as u16).to_be_bytes());
    for s in suites {
        hello.extend_from_slice(&s.to_be_bytes());
    }
    hello.extend_from_slice(&[1, 0]);
    let base = 5 + 4 + hello.len() + 2 + ext.len();
    if base + 4 < 517 {
        let pad = vec![0u8; 517 - base - 4];
        put_ext(&mut ext, 0x0015, &pad);
    }
    hello.extend_from_slice(&(ext.len() as u16).to_be_bytes());
    hello.extend_from_slice(&ext);

    let mut hs = vec![0x01];
    hs.extend_from_slice(&(hello.len() as u32).to_be_bytes()[1..]);
    hs.extend_from_slice(&hello);
    let mut rec = vec![0x16, 0x03, 0x01];
    rec.extend_from_slice(&(hs.len() as u16).to_be_bytes());
    rec.extend_from_slice(&hs);
    rec
}

pub fn fake_http(host: &str) -> Vec<u8> {
    format!("GET / HTTP/1.1\r\nHost: {host}\r\nUser-Agent: Mozilla/5.0\r\nAccept: */*\r\n\r\n").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tcp4(payload: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 40];
        p[0] = 0x45;
        p[8] = 64;
        p[9] = PROTO_TCP;
        p[12..16].copy_from_slice(&[192, 168, 1, 2]);
        p[16..20].copy_from_slice(&[1, 2, 3, 4]);
        p[20..22].copy_from_slice(&50000u16.to_be_bytes());
        p[22..24].copy_from_slice(&443u16.to_be_bytes());
        p[24..28].copy_from_slice(&1000u32.to_be_bytes());
        p[32] = 0x50;
        p[33] = TCP_ACK | TCP_PSH;
        p.extend_from_slice(payload);
        fix(&mut p);
        p
    }

    #[test]
    fn sni_from_fake() {
        let ch = fake_client_hello("www.example.com");
        assert_eq!(ch.len(), 517);
        let h = tls_sni(&ch).unwrap();
        assert_eq!(h.host, "www.example.com");
        assert_eq!(&ch[h.offset..h.offset + 15], b"www.example.com");
    }

    #[test]
    fn http_host_parse() {
        let mut r = b"GET / HTTP/1.1\r\nHOST:  Discord.com:80\r\nA: b\r\n\r\n".to_vec();
        let h = http_host(&r).unwrap();
        assert_eq!(h.host, "discord.com");
        assert_eq!(&r[h.offset..h.offset + 7], b"Discord");
        assert!(http_mix_host(&mut r));
        assert!(r.starts_with(b"GET / HTTP/1.1\r\nhoSt:"));
    }

    #[test]
    fn checksum_and_segment() {
        let pl = fake_client_hello("a.com");
        let p = tcp4(&pl);
        let ip = parse(&p).unwrap();
        assert_eq!(ip.payload(), 40);
        assert_eq!(fold(sum16(&p[..20], 0)), 0);
        let s = segment(&p, &ip, &pl[..2], tcp_seq(&p, &ip));
        let s2 = segment(&p, &ip, &pl[2..], tcp_seq(&p, &ip) + 2);
        let i2 = parse(&s2).unwrap();
        assert_eq!(s.len(), 42);
        assert_eq!(tcp_seq(&s2, &i2), 1002);
        let mut acc = sum16(&s2[12..20], 0) + PROTO_TCP as u32 + (s2.len() - 20) as u32;
        acc = sum16(&s2[20..], acc);
        assert_eq!(fold(acc), 0);
    }
}
