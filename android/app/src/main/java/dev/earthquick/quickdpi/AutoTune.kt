package dev.earthquick.quickdpi

import java.net.HttpURLConnection
import java.net.InetSocketAddress
import java.net.Proxy
import java.net.URL
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

object AutoTune {
    data class State(val running: Boolean = false, val step: String = "", val baseline: Int? = null, val results: List<Triple<String, Int, Int>> = emptyList(), val message: String? = null)

    @Volatile var state = State()
        private set

    private fun probe(site: String, proxy: Proxy): Boolean = runCatching {
        val c = URL("https://$site/").openConnection(proxy) as HttpURLConnection
        c.connectTimeout = 7000
        c.readTimeout = 7000
        c.instanceFollowRedirects = false
        c.setRequestProperty("User-Agent", "Mozilla/5.0 (QuickDPI)")
        try { c.responseCode > 0 } finally { c.disconnect() }
    }.getOrDefault(false)

    private fun probeAll(sites: List<String>, proxy: Proxy): Int {
        val pool = Executors.newFixedThreadPool(sites.size.coerceIn(1, 6))
        val fs = sites.map { s -> pool.submit<Boolean> { probe(s, proxy) } }
        val ok = fs.count { runCatching { it.get(20, TimeUnit.SECONDS) }.getOrDefault(false) }
        pool.shutdownNow()
        return ok
    }

    fun start(prefs: Prefs, onDone: () -> Unit) {
        if (state.running) return
        state = State(running = true, step = t("Hazırlanıyor", "Preparing"))
        Thread {
            val sites = prefs.testSites.map(::normalizeRule).filter { it.isNotEmpty() }
            if (sites.isEmpty()) {
                state = State(message = t("Test sitesi yok: Gelişmiş bölümünden ekleyin.", "No test sites: add some under Advanced."))
                onDone(); return@Thread
            }
            val total = sites.size
            state = state.copy(step = t("Koruma olmadan deneniyor", "Testing without protection"))
            val baseline = probeAll(sites, Proxy.NO_PROXY)
            state = state.copy(baseline = baseline)
            val dns = prefs.dns?.let { DnsClient(it) }
            var best: Pair<Int, Strategy>? = null
            val results = ArrayList<Triple<String, Int, Int>>()
            for (s in STRATEGIES) {
                state = state.copy(step = s.name)
                val p = LocalProxy({ s }, { RuleMode.EXCEPT }, { emptyList() }, { dns }).start()
                val ok = probeAll(sites, Proxy(Proxy.Type.HTTP, InetSocketAddress("127.0.0.1", p.port)))
                p.stop()
                results += Triple(s.name, ok, total)
                state = state.copy(results = results.toList())
                if (best == null || ok > best.first) best = ok to s
                if (ok == total) break
            }
            val (ok, s) = best!!
            if (ok > 0) prefs.tuned = s.id
            val msg = when {
                ok == 0 -> t("Hiçbir ayar test sitelerini açamadı. Bağlantınızı ve test sitelerini kontrol edin.", "No setting could open the test sites. Check your connection and the test sites.")
                baseline == total -> t("Siteler koruma olmadan da açılıyordu. Yine de seçildi: ", "Sites opened without protection too. Selected anyway: ") + s.name
                else -> t("Seçildi: ", "Selected: ") + "${s.name} ($ok/$total)"
            }
            state = state.copy(running = false, step = "", message = msg)
            onDone()
        }.start()
    }
}
