package dev.earthquick.quickdpi

import java.io.ByteArrayOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.net.InetSocketAddress
import java.util.concurrent.ConcurrentHashMap
import kotlin.random.Random

class DnsClient(private val preset: DnsPreset) {
    private val servers: List<InetSocketAddress> = listOfNotNull(preset.v4, preset.v6).map { InetSocketAddress(InetAddress.getByName(it), preset.port) }
    private val cache = ConcurrentHashMap<String, Pair<Long, List<InetAddress>>>()

    fun forward(query: ByteArray): ByteArray? {
        if (query.size < 12) return null
        for (srv in servers) {
            repeat(2) {
                runCatching {
                    DatagramSocket().use { s ->
                        s.soTimeout = 2500
                        s.send(DatagramPacket(query, query.size, srv))
                        val buf = ByteArray(4096)
                        val deadline = System.currentTimeMillis() + 2500
                        while (System.currentTimeMillis() < deadline) {
                            val pk = DatagramPacket(buf, buf.size)
                            s.receive(pk)
                            if (pk.length >= 2 && buf[0] == query[0] && buf[1] == query[1]) return buf.copyOf(pk.length)
                        }
                    }
                }
            }
        }
        return null
    }

    fun resolve(host: String): List<InetAddress> {
        literalIp(host)?.let { return listOf(it) }
        val key = host.lowercase()
        cache[key]?.let { (until, ips) -> if (System.currentTimeMillis() < until && ips.isNotEmpty()) return ips }
        var ttl = 300
        val ips = ArrayList<InetAddress>()
        for (type in intArrayOf(1, 28)) {
            val resp = forward(query(key, type)) ?: continue
            val (list, t) = parseAnswers(resp, type)
            ips += list
            ttl = minOf(ttl, maxOf(t, 30))
            if (ips.isNotEmpty()) break
        }
        if (ips.isNotEmpty()) cache[key] = (System.currentTimeMillis() + ttl * 1000L) to ips
        return ips
    }

    companion object {
        fun query(host: String, type: Int): ByteArray {
            val o = ByteArrayOutputStream()
            val id = Random.nextInt(65536)
            o.write(byteArrayOf((id shr 8).toByte(), id.toByte(), 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0))
            for (label in host.trimEnd('.').split('.')) {
                val b = label.toByteArray()
                o.write(b.size)
                o.write(b)
            }
            o.write(byteArrayOf(0, (type shr 8).toByte(), type.toByte(), 0, 1))
            return o.toByteArray()
        }

        private fun u16(b: ByteArray, i: Int) = ((b[i].toInt() and 0xff) shl 8) or (b[i + 1].toInt() and 0xff)

        private fun skipName(b: ByteArray, start: Int): Int {
            var i = start
            while (i < b.size) {
                val len = b[i].toInt() and 0xff
                if (len == 0) return i + 1
                if (len and 0xc0 == 0xc0) return i + 2
                i += len + 1
            }
            return i
        }

        fun parseAnswers(b: ByteArray, type: Int): Pair<List<InetAddress>, Int> {
            val out = ArrayList<InetAddress>()
            var ttl = 300
            runCatching {
                val qd = u16(b, 4)
                val an = u16(b, 6)
                var i = 12
                repeat(qd) { i = skipName(b, i) + 4 }
                repeat(an) {
                    i = skipName(b, i)
                    val typ = u16(b, i)
                    val t = (u16(b, i + 4) shl 16) or u16(b, i + 6)
                    val len = u16(b, i + 8)
                    i += 10
                    if (typ == type && (len == 4 || len == 16)) {
                        out += InetAddress.getByAddress(b.copyOfRange(i, i + len))
                        ttl = minOf(ttl, t)
                    }
                    i += len
                }
            }
            return out to ttl
        }
    }
}
