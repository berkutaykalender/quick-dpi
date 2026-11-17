package dev.earthquick.quickdpi

object Packets {
    class Query(val v6: Boolean, val src: ByteArray, val dst: ByteArray, val srcPort: Int, val dstPort: Int, val payload: ByteArray)

    private fun u16(b: ByteArray, i: Int) = ((b[i].toInt() and 0xff) shl 8) or (b[i + 1].toInt() and 0xff)

    fun dnsQuery(p: ByteArray): Query? {
        if (p.isEmpty()) return null
        return when (p[0].toInt() ushr 4) {
            4 -> {
                if (p.size < 28) return null
                val ihl = (p[0].toInt() and 0x0f) * 4
                if (p[9].toInt() != 17 || u16(p, 6) and 0x3fff != 0) return null
                val total = minOf(u16(p, 2), p.size)
                if (u16(p, ihl + 2) != 53 || total < ihl + 8) return null
                Query(false, p.copyOfRange(12, 16), p.copyOfRange(16, 20), u16(p, ihl), 53, p.copyOfRange(ihl + 8, total))
            }
            6 -> {
                if (p.size < 48 || p[6].toInt() != 17) return null
                if (u16(p, 42) != 53) return null
                val total = minOf(40 + u16(p, 4), p.size)
                Query(true, p.copyOfRange(8, 24), p.copyOfRange(24, 40), u16(p, 40), 53, p.copyOfRange(48, total))
            }
            else -> null
        }
    }

    private fun sum(b: ByteArray, from: Int, to: Int, start: Long = 0): Long {
        var acc = start
        var i = from
        while (i + 1 < to) {
            acc += u16(b, i); i += 2
        }
        if (i < to) acc += (b[i].toInt() and 0xff) shl 8
        return acc
    }

    private fun fold(a: Long): Int {
        var x = a
        while (x shr 16 != 0L) x = (x and 0xffff) + (x shr 16)
        return x.toInt().inv() and 0xffff
    }

    private fun put16(b: ByteArray, i: Int, v: Int) {
        b[i] = (v shr 8).toByte(); b[i + 1] = v.toByte()
    }

    fun reply(q: Query, dns: ByteArray): ByteArray {
        val udpLen = 8 + dns.size
        return if (!q.v6) {
            val p = ByteArray(20 + udpLen)
            p[0] = 0x45; put16(p, 2, p.size); p[6] = 0x40; p[8] = 64; p[9] = 17
            q.dst.copyInto(p, 12); q.src.copyInto(p, 16)
            put16(p, 10, fold(sum(p, 0, 20)))
            put16(p, 20, q.dstPort); put16(p, 22, q.srcPort); put16(p, 24, udpLen)
            dns.copyInto(p, 28)
            p
        } else {
            val p = ByteArray(40 + udpLen)
            p[0] = 0x60; put16(p, 4, udpLen); p[6] = 17; p[7] = 64
            q.dst.copyInto(p, 8); q.src.copyInto(p, 24)
            put16(p, 40, q.dstPort); put16(p, 42, q.srcPort); put16(p, 44, udpLen)
            dns.copyInto(p, 48)
            val pseudo = sum(p, 8, 40) + 17 + udpLen
            var c = fold(sum(p, 40, p.size, pseudo))
            if (c == 0) c = 0xffff
            put16(p, 46, c)
            p
        }
    }
}
