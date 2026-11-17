package dev.earthquick.quickdpi

import android.content.Context
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.util.Locale

fun t(tr: String, en: String): String = if (Locale.getDefault().language == "tr") tr else en

data class Strategy(
    val id: String,
    val nameTr: String,
    val nameEn: String,
    val tlsRecord: Boolean,
    val split: Int,
) {
    val name get() = t(nameTr, nameEn)
}

val STRATEGIES = listOf(
    Strategy("split2", "Bölme (2. bayt)", "Split (2nd byte)", false, 2),
    Strategy("tlsrec", "TLS kaydı bölme", "TLS record split", true, -1),
    Strategy("split1", "Bölme (1. bayt)", "Split (1st byte)", false, 1),
    Strategy("tlsrec2", "TLS kaydı + bölme (2. bayt)", "TLS record + split (2nd byte)", true, 2),
    Strategy("splithost", "Site adının ortasından bölme", "Split in the middle of the hostname", false, -1),
)

fun strategy(id: String?): Strategy = STRATEGIES.firstOrNull { it.id == id } ?: STRATEGIES[0]

data class DnsPreset(val id: String, val name: String, val v4: String?, val v6: String?, val port: Int) {
    val altPort get() = port != 53
    val label get() = "$name — ${v4 ?: v6}:$port"
}

val DNS_PRESETS = listOf(
    DnsPreset("yandex", "Yandex", "77.88.8.8", "2a02:6b8::feed:0ff", 1253),
    DnsPreset("yandex-2", "Yandex (2)", "77.88.8.1", null, 1253),
    DnsPreset("adguard", "AdGuard", "94.140.14.14", null, 5353),
    DnsPreset("adguard-2", "AdGuard (2)", "94.140.15.15", null, 5353),
    DnsPreset("opendns", "OpenDNS (Cisco)", "208.67.222.222", null, 5353),
    DnsPreset("opendns-2", "OpenDNS (Cisco, 2)", "208.67.220.220", null, 5353),
    DnsPreset("cleanbrowsing", "CleanBrowsing", "185.228.168.9", null, 5353),
    DnsPreset("nextdns", "NextDNS", "45.90.28.0", null, 5353),
    DnsPreset("safedns", "SafeDNS", "195.46.39.39", null, 1253),
    DnsPreset("cloudflare", "Cloudflare", "1.1.1.1", "2606:4700:4700::1111", 53),
    DnsPreset("cloudflare-2", "Cloudflare (2)", "1.0.0.1", "2606:4700:4700::1001", 53),
    DnsPreset("google", "Google", "8.8.8.8", "2001:4860:4860::8888", 53),
    DnsPreset("google-2", "Google (2)", "8.8.4.4", "2001:4860:4860::8844", 53),
    DnsPreset("quad9", "Quad9", "9.9.9.9", "2620:fe::fe", 53),
    DnsPreset("quad9-2", "Quad9 (2)", "149.112.112.112", "2620:fe::9", 53),
    DnsPreset("opendns-53", "OpenDNS (Cisco)", "208.67.222.222", "2620:119:35::35", 53),
    DnsPreset("adguard-53", "AdGuard", "94.140.14.14", "2a10:50c0::ad1:ff", 53),
    DnsPreset("controld", "Control D", "76.76.2.0", null, 53),
    DnsPreset("comodo", "Comodo Secure DNS", "8.26.56.26", null, 53),
    DnsPreset("ultradns", "UltraDNS (Vercara)", "156.154.70.1", null, 53),
    DnsPreset("dnswatch", "DNS.Watch", "84.200.69.80", null, 53),
)

fun parseCustomDns(s: String): DnsPreset? {
    val x = s.trim()
    if (x.isEmpty()) return null
    val (host, port) = when {
        x.startsWith("[") -> x.substringAfter("[").substringBefore("]") to (x.substringAfter("]:", "53").toIntOrNull() ?: return null)
        x.count { it == ':' } == 1 -> x.substringBefore(":") to (x.substringAfter(":").toIntOrNull() ?: return null)
        else -> x to 53
    }
    if (port !in 1..65535) return null
    val ip = literalIp(host) ?: return null
    return if (ip is Inet4Address) DnsPreset("custom", t("Özel", "Custom"), host, null, port) else DnsPreset("custom", t("Özel", "Custom"), null, host, port)
}

fun literalIp(s: String): InetAddress? {
    val ok = s.matches(Regex("^[0-9.]+$")) && s.split('.').size == 4 || (s.contains(':') && s.matches(Regex("^[0-9a-fA-F:.]+$")))
    return if (ok) runCatching { InetAddress.getByName(s) }.getOrNull() else null
}

enum class RuleMode { EXCEPT, ONLY }

class Prefs(ctx: Context) {
    private val p = ctx.applicationContext.getSharedPreferences("quickdpi", Context.MODE_PRIVATE)

    var enabled: Boolean
        get() = p.getBoolean("enabled", false)
        set(v) = p.edit().putBoolean("enabled", v).apply()
    var strategyChoice: String
        get() = p.getString("strategy", "auto")!!
        set(v) = p.edit().putString("strategy", v).apply()
    var tuned: String?
        get() = p.getString("tuned", null)
        set(v) = p.edit().putString("tuned", v).apply()
    var dnsEnabled: Boolean
        get() = p.getBoolean("dns", true)
        set(v) = p.edit().putBoolean("dns", v).apply()
    var dnsId: String
        get() = p.getString("dnsId", "yandex")!!
        set(v) = p.edit().putString("dnsId", v).apply()
    var dnsCustom: String
        get() = p.getString("dnsCustom", "")!!
        set(v) = p.edit().putString("dnsCustom", v).apply()
    var ruleMode: RuleMode
        get() = if (p.getString("ruleMode", "except") == "only") RuleMode.ONLY else RuleMode.EXCEPT
        set(v) = p.edit().putString("ruleMode", if (v == RuleMode.ONLY) "only" else "except").apply()
    var rules: List<String>
        get() = p.getString("rules", "")!!.lines().filter { it.isNotBlank() }
        set(v) = p.edit().putString("rules", v.joinToString("\n")).apply()
    var testSites: List<String>
        get() = p.getString("testSites", null)?.lines()?.filter { it.isNotBlank() } ?: listOf("discord.com", "www.roblox.com", "www.wattpad.com")
        set(v) = p.edit().putString("testSites", v.joinToString("\n")).apply()
    var acceptedTerms: Int
        get() = p.getInt("acceptedTerms", 0)
        set(v) = p.edit().putInt("acceptedTerms", v).apply()
    val accepted get() = acceptedTerms >= Texts.TERMS_VERSION
    var askedNotify: Boolean
        get() = p.getBoolean("askedNotify", false)
        set(v) = p.edit().putBoolean("askedNotify", v).apply()

    val activeStrategy: Strategy get() = strategy(if (strategyChoice == "auto") tuned else strategyChoice)

    val dns: DnsPreset?
        get() = if (!dnsEnabled) null else if (dnsId == "custom") parseCustomDns(dnsCustom) else DNS_PRESETS.firstOrNull { it.id == dnsId } ?: DNS_PRESETS[0]
}

fun normalizeRule(r: String): String {
    var s = r.trim().lowercase().removePrefix("https://").removePrefix("http://").substringBefore('/')
    s = s.removePrefix("*.").trimStart('.').trimEnd('.')
    return s
}

fun ruleMatches(rule: String, host: String?): Boolean = host != null && (host == rule || host.endsWith(".$rule"))

fun applies(mode: RuleMode, rules: List<String>, host: String?): Boolean {
    val hit = rules.any { ruleMatches(it, host) }
    return if (mode == RuleMode.EXCEPT) !hit else hit
}

fun isV4(a: InetAddress) = a is Inet4Address
fun isV6(a: InetAddress) = a is Inet6Address
