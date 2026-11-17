package dev.earthquick.quickdpi

import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicLong

object Stats {
    val connections = AtomicLong()
    val protected = AtomicLong()
    val excluded = AtomicLong()
    val dns = AtomicLong()
    private val recent = ArrayDeque<Pair<String, Boolean>>()

    fun remember(host: String, applied: Boolean) = synchronized(recent) {
        recent.removeAll { it.first == host }
        recent.addFirst(host to applied)
        while (recent.size > 40) recent.removeLast()
    }

    fun recent(): List<Pair<String, Boolean>> = synchronized(recent) { recent.toList() }

    fun reset() {
        connections.set(0); protected.set(0); excluded.set(0); dns.set(0)
        synchronized(recent) { recent.clear() }
    }
}

object Hello {
    fun isClientHello(d: ByteArray, n: Int) = n > 5 && d[0].toInt() == 0x16 && d[1].toInt() == 0x03 && d[5].toInt() == 0x01

    private fun u16(d: ByteArray, i: Int) = ((d[i].toInt() and 0xff) shl 8) or (d[i + 1].toInt() and 0xff)

    fun sni(d: ByteArray, n: Int): Pair<Int, Int>? = runCatching {
        if (!isClientHello(d, n)) return null
        var i = 5 + 4 + 2 + 32
        i += 1 + (d[i].toInt() and 0xff)
        i += 2 + u16(d, i)
        i += 1 + (d[i].toInt() and 0xff)
        i += 2
        while (i + 4 <= n) {
            val typ = u16(d, i)
            val len = u16(d, i + 2)
            if (typ == 0) {
                val nameLen = u16(d, i + 4 + 3)
                val start = i + 4 + 5
                if (start + nameLen > n) return null
                return start to nameLen
            }
            i += 4 + len
        }
        null
    }.getOrNull()

    fun isHttp(d: ByteArray, n: Int): Boolean {
        val s = String(d, 0, minOf(n, 8), Charsets.ISO_8859_1)
        return listOf("GET ", "POST ", "HEAD ", "PUT ", "OPTIONS ", "DELETE ", "PATCH ").any { s.startsWith(it) }
    }

    fun pieces(d: ByteArray, n: Int, s: Strategy): List<ByteArray> {
        val data = d.copyOf(n)
        val tls = isClientHello(d, n)
        val host = if (tls) sni(d, n) else null
        var bytes = data
        if (tls && s.tlsRecord && n > 10) {
            val bodyLen = u16(d, 3)
            val k = (host?.let { it.first - 5 + it.second / 2 } ?: 3).coerceIn(1, minOf(bodyLen, n - 5) - 1)
            val r1 = byteArrayOf(0x16, d[1], d[2], (k shr 8).toByte(), k.toByte()) + data.copyOfRange(5, 5 + k)
            val rest = bodyLen - k
            val r2 = byteArrayOf(0x16, d[1], d[2], (rest shr 8).toByte(), rest.toByte()) + data.copyOfRange(5 + k, n)
            bytes = r1 + r2
            if (s.split == -1) return listOf(r1, r2)
        }
        val pos = when {
            s.split > 0 -> s.split
            s.split == -1 && host != null -> host.first + host.second / 2
            s.split == -1 -> 2
            else -> 0
        }
        if (pos <= 0 || pos >= bytes.size) return listOf(bytes)
        return listOf(bytes.copyOfRange(0, pos), bytes.copyOfRange(pos, bytes.size))
    }
}

class LocalProxy(
    private val strategyOf: () -> Strategy,
    private val ruleModeOf: () -> RuleMode,
    private val rulesOf: () -> List<String>,
    private val dnsOf: () -> DnsClient?,
) {
    private val server = ServerSocket(0, 256, InetAddress.getByName("127.0.0.1"))
    private val pool = Executors.newCachedThreadPool { r -> Thread(r, "qd-proxy").apply { isDaemon = true } }
    @Volatile private var closed = false
    val port: Int get() = server.localPort

    fun start(): LocalProxy {
        Thread({
            while (!closed) {
                val c = runCatching { server.accept() }.getOrNull() ?: continue
                pool.execute { runCatching { handle(c) }; runCatching { c.close() } }
            }
        }, "qd-accept").apply { isDaemon = true }.start()
        return this
    }

    fun stop() {
        closed = true
        runCatching { server.close() }
        pool.shutdownNow()
    }

    private fun resolve(host: String): List<InetAddress> {
        literalIp(host)?.let { return listOf(it) }
        val dns = dnsOf()
        val ips = dns?.resolve(host) ?: InetAddress.getAllByName(host).toList()
        return ips.sortedBy { if (isV4(it)) 0 else 1 }
    }

    private fun connect(host: String, port: Int): Socket {
        var last: Exception? = null
        for (ip in resolve(host).take(4)) {
            try {
                val s = Socket()
                s.tcpNoDelay = true
                s.connect(InetSocketAddress(ip, port), 6000)
                return s
            } catch (e: Exception) {
                last = e
            }
        }
        throw last ?: java.net.UnknownHostException(host)
    }

    private fun readHead(inp: InputStream): Pair<String, ByteArray>? {
        val buf = ByteArrayOutputStream()
        val b = ByteArray(8192)
        while (buf.size() < 65536) {
            val n = inp.read(b)
            if (n <= 0) return null
            buf.write(b, 0, n)
            val all = buf.toByteArray()
            val end = indexOf(all, "\r\n\r\n".toByteArray())
            if (end >= 0) return String(all, 0, end + 4, Charsets.ISO_8859_1) to all.copyOfRange(end + 4, all.size)
        }
        return null
    }

    private fun indexOf(h: ByteArray, n: ByteArray): Int {
        outer@ for (i in 0..h.size - n.size) {
            for (j in n.indices) if (h[i + j] != n[j]) continue@outer
            return i
        }
        return -1
    }

    private fun writePieces(out: OutputStream, parts: List<ByteArray>) {
        parts.forEachIndexed { i, p ->
            out.write(p)
            out.flush()
            if (i < parts.size - 1) Thread.sleep(8)
        }
    }

    private fun handle(client: Socket) {
        client.tcpNoDelay = true
        client.soTimeout = 30000
        val cin = client.getInputStream()
        val cout = client.getOutputStream()
        val (head, extra) = readHead(cin) ?: return
        val line = head.substringBefore("\r\n").split(' ')
        if (line.size < 3) return
        val method = line[0]
        Stats.connections.incrementAndGet()
        if (method.equals("CONNECT", true)) {
            val target = line[1]
            val host = if (target.startsWith("[")) target.substringAfter("[").substringBefore("]") else target.substringBeforeLast(':')
            val port = target.substringAfterLast(':').toIntOrNull() ?: 443
            val remote = try {
                connect(host, port)
            } catch (e: Exception) {
                cout.write("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n".toByteArray()); return
            }
            remote.use {
                cout.write("HTTP/1.1 200 Connection established\r\n\r\n".toByteArray())
                cout.flush()
                val first = if (extra.isNotEmpty()) extra else {
                    val b = ByteArray(16384)
                    val n = cin.read(b)
                    if (n <= 0) return
                    b.copyOf(n)
                }
                sendFirst(host.lowercase(), first, remote.getOutputStream())
                pipe(client, remote)
            }
        } else {
            val url = line[1]
            if (!url.startsWith("http://", true)) return
            val rest = url.substring(7)
            val hostPort = rest.substringBefore('/')
            val path = "/" + rest.substringAfter('/', "")
            val host = hostPort.substringBefore(':')
            val port = hostPort.substringAfter(':', "80").toIntOrNull() ?: 80
            val headers = head.split("\r\n").drop(1).filter { it.isNotEmpty() && !it.startsWith("Proxy-Connection", true) && !it.startsWith("Connection", true) }
            val req = "$method $path ${line[2]}\r\n" + headers.joinToString("") { "$it\r\n" } + "Connection: close\r\n\r\n"
            val remote = try {
                connect(host, port)
            } catch (e: Exception) {
                cout.write("HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n".toByteArray()); return
            }
            remote.use {
                sendFirst(host.lowercase(), req.toByteArray(Charsets.ISO_8859_1) + extra, remote.getOutputStream())
                pipe(client, remote)
            }
        }
    }

    private fun sendFirst(host: String, data: ByteArray, out: OutputStream) {
        var d = data
        val apply = applies(ruleModeOf(), rulesOf(), host)
        Stats.remember(host, apply)
        if (!apply) {
            Stats.excluded.incrementAndGet()
            out.write(d); out.flush(); return
        }
        Stats.protected.incrementAndGet()
        if (Hello.isHttp(d, d.size)) {
            val s = String(d, Charsets.ISO_8859_1)
            val i = s.indexOf("\r\nHost:", ignoreCase = true)
            if (i >= 0) d = (s.substring(0, i + 2) + "hoSt" + s.substring(i + 6)).toByteArray(Charsets.ISO_8859_1)
        }
        writePieces(out, Hello.pieces(d, d.size, strategyOf()))
    }

    private fun pipe(a: Socket, b: Socket) {
        a.soTimeout = 0
        b.soTimeout = 0
        val t = Thread({ copy(b.getInputStream(), a.getOutputStream()); runCatching { a.shutdownOutput() } }, "qd-down").apply { isDaemon = true }
        t.start()
        copy(a.getInputStream(), b.getOutputStream())
        runCatching { b.shutdownOutput() }
        t.join()
    }

    private fun copy(i: InputStream, o: OutputStream) {
        val buf = ByteArray(32768)
        try {
            while (true) {
                val n = i.read(buf)
                if (n < 0) break
                o.write(buf, 0, n)
                o.flush()
            }
        } catch (_: Exception) {
        }
    }
}
