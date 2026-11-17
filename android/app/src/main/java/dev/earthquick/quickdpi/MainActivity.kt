package dev.earthquick.quickdpi

import android.Manifest
import android.app.Activity
import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.VpnService
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.PopupMenu
import android.widget.RadioButton
import android.widget.RadioGroup
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView
import android.widget.Toast

class MainActivity : Activity() {
    private lateinit var prefs: Prefs
    private val ui = Handler(Looper.getMainLooper())
    private var dark = false

    private val accent = Color.rgb(38, 132, 255)
    private val green = Color.rgb(46, 170, 90)
    private val bg get() = if (dark) Color.rgb(18, 19, 22) else Color.rgb(244, 245, 248)
    private val card get() = if (dark) Color.rgb(30, 32, 37) else Color.WHITE
    private val fg get() = if (dark) Color.rgb(232, 234, 238) else Color.rgb(25, 27, 32)
    private val dim get() = if (dark) Color.rgb(150, 154, 162) else Color.rgb(105, 110, 120)

    private lateinit var power: TextView
    private lateinit var status: TextView
    private lateinit var counters: TextView
    private lateinit var strategyText: TextView
    private lateinit var tuneText: TextView
    private lateinit var tuneButton: Button
    private lateinit var dnsSwitch: Switch
    private lateinit var dnsButton: Button
    private lateinit var rulesBox: LinearLayout
    private lateinit var recentBox: LinearLayout
    private lateinit var advStrategy: Button
    private lateinit var updText: TextView
    private lateinit var updButton: Button
    private var shownRecent: List<Pair<String, Boolean>>? = null
    private var pendingStart = false

    private val tick = object : Runnable {
        override fun run() {
            refresh()
            ui.postDelayed(this, 1000)
        }
    }

    private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        prefs = Prefs(this)
        actionBar?.hide()
        dark = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
        window.statusBarColor = bg
        window.navigationBarColor = bg
        if (!dark) window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
        setContentView(build())
        if (prefs.accepted) afterAccept(intent?.getBooleanExtra("start", false) == true)
        else showTerms(true, intent?.getBooleanExtra("start", false) == true)
    }

    private fun afterAccept(start: Boolean) {
        askNotify()
        if (prefs.tuned == null && !AutoTune.state.running) runTune()
        if (start) turnOn()
    }

    private fun bullets(items: List<String>, size: Float = 14f): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        for (s in items) addView(text("•  $s", size).apply { setPadding(0, dp(4), 0, dp(4)) })
    }

    private fun showTerms(must: Boolean, startAfter: Boolean = false) {
        val box = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(dp(22), dp(8), dp(22), 0) }
        box.addView(text(Texts.termsIntro, 15f))
        box.addView(bullets(Texts.terms).apply { setPadding(0, dp(8), 0, dp(8)) })
        val check = android.widget.CheckBox(this).apply { text = Texts.termsCheck; setTextColor(fg) }
        if (must) box.addView(check)
        val b = AlertDialog.Builder(this).setTitle(Texts.termsTitle).setView(ScrollView(this).apply { addView(box) })
        if (!must) {
            b.setPositiveButton(t("Kapat", "Close"), null).show(); return
        }
        val dlg = b.setCancelable(false)
            .setPositiveButton(Texts.termsAccept, null)
            .setNegativeButton(Texts.termsDecline) { _, _ -> finishAndRemoveTask() }
            .create()
        dlg.setOnShowListener {
            val ok = dlg.getButton(AlertDialog.BUTTON_POSITIVE)
            ok.isEnabled = false
            check.setOnCheckedChangeListener { _, c -> ok.isEnabled = c }
            ok.setOnClickListener {
                prefs.acceptedTerms = Texts.TERMS_VERSION
                dlg.dismiss()
                afterAccept(startAfter)
            }
        }
        dlg.show()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.getBooleanExtra("start", false) && prefs.accepted) turnOn()
    }

    override fun onResume() {
        super.onResume()
        Updater.checkIfDue(this)
        ui.post(tick)
    }

    override fun onPause() {
        ui.removeCallbacks(tick)
        super.onPause()
    }

    private fun text(s: String, size: Float = 15f, color: Int = fg, bold: Boolean = false) = TextView(this).apply {
        text = s
        textSize = size
        setTextColor(color)
        if (bold) typeface = Typeface.DEFAULT_BOLD
    }

    private fun button(s: String, onClick: () -> Unit) = Button(this).apply {
        text = s
        isAllCaps = false
        setOnClickListener { onClick() }
    }

    private fun card(title: String, body: LinearLayout.() -> Unit): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(dp(16), dp(14), dp(16), dp(14))
        background = GradientDrawable().apply { setColor(card); cornerRadius = dp(14).toFloat() }
        layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(12) }
        addView(text(title, 17f, fg, true))
        body()
    }

    private fun build(): View {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(16), dp(20), dp(16), dp(28))
        }
        root.addView(LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            addView(android.widget.ImageView(this@MainActivity).apply {
                setImageResource(R.mipmap.ic_launcher)
                layoutParams = LinearLayout.LayoutParams(dp(48), dp(48)).apply { marginEnd = dp(12) }
            })
            addView(LinearLayout(this@MainActivity).apply {
                orientation = LinearLayout.VERTICAL
                addView(text("QuickDPI", 24f, fg, true))
                addView(text(t("Ağ güvenliği", "Network security"), 14f, dim))
            })
        })

        power = TextView(this).apply {
            gravity = Gravity.CENTER
            textSize = 20f
            typeface = Typeface.DEFAULT_BOLD
            setTextColor(Color.WHITE)
            layoutParams = LinearLayout.LayoutParams(dp(150), dp(150)).apply { gravity = Gravity.CENTER_HORIZONTAL; topMargin = dp(20) }
            setOnClickListener { if (QuickVpnService.running) QuickVpnService.stop(this@MainActivity) else turnOn() }
        }
        root.addView(power)
        status = text("", 16f, fg, true).apply {
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(12) }
        }
        root.addView(status)
        counters = text("", 13f, dim).apply { gravity = Gravity.CENTER }
        root.addView(counters)

        root.addView(card(t("Ayar", "Setting")) {
            strategyText = text("", 15f).also { addView(it) }
            tuneText = text("", 13f, dim).also { addView(it) }
            tuneButton = button(t("Otomatik ayarla", "Auto-configure")) { runTune() }.also { addView(it) }
        })

        root.addView(card(t("Ağ güvenliği", "Network security")) {
            dnsSwitch = Switch(this@MainActivity).apply {
                text = t("Güvenli DNS", "Secure DNS")
                setTextColor(fg)
                textSize = 15f
                isChecked = prefs.dnsEnabled
                setOnCheckedChangeListener { _, on -> prefs.dnsEnabled = on; changed() }
            }
            addView(dnsSwitch)
            dnsButton = button("") { dnsDialog() }.also { addView(it) }
        })

        root.addView(card(t("Site kuralları", "Site rules")) {
            val group = RadioGroup(this@MainActivity)
            val except = RadioButton(this@MainActivity).apply { id = View.generateViewId(); text = t("Bu siteler hariç hepsine uygula", "Apply to all except these"); setTextColor(fg) }
            val only = RadioButton(this@MainActivity).apply { id = View.generateViewId(); text = t("Yalnız bu sitelere uygula", "Apply only to these"); setTextColor(fg) }
            group.addView(except); group.addView(only)
            group.check(if (prefs.ruleMode == RuleMode.EXCEPT) except.id else only.id)
            group.setOnCheckedChangeListener { _, id -> prefs.ruleMode = if (id == only.id) RuleMode.ONLY else RuleMode.EXCEPT; changed() }
            addView(group)
            val row = LinearLayout(this@MainActivity).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
            val input = EditText(this@MainActivity).apply {
                hint = t("ornek.com", "example.com")
                setTextColor(fg); setHintTextColor(dim)
                inputType = InputType.TYPE_TEXT_VARIATION_URI
                imeOptions = EditorInfo.IME_ACTION_DONE
                isSingleLine = true
                layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            }
            val add = {
                val r = normalizeRule(input.text.toString())
                if (r.isNotEmpty()) { addRule(r); input.setText("") }
            }
            input.setOnEditorActionListener { _, _, _ -> add(); true }
            row.addView(input)
            row.addView(button(t("Ekle", "Add")) { add() })
            addView(row)
            rulesBox = LinearLayout(this@MainActivity).apply { orientation = LinearLayout.VERTICAL }
            addView(rulesBox)
        })

        root.addView(card(t("Son siteler", "Recent sites")) {
            addView(text(t("Dokunarak kurallara ekleyin ya da çıkarın.", "Tap to add to or remove from the rules."), 13f, dim))
            recentBox = LinearLayout(this@MainActivity).apply { orientation = LinearLayout.VERTICAL }
            addView(recentBox)
        })

        root.addView(card(t("Gelişmiş", "Advanced")) {
            addView(text(t("Yöntem", "Method"), 13f, dim))
            advStrategy = button("") { strategyDialog() }.also { addView(it) }
            addView(text(t("Test siteleri (otomatik ayar için, satır satır)", "Test sites (for auto-configure, one per line)"), 13f, dim))
            val sites = EditText(this@MainActivity).apply {
                setText(prefs.testSites.joinToString("\n"))
                setTextColor(fg)
                inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE or InputType.TYPE_TEXT_VARIATION_URI
                minLines = 3
            }
            addView(sites)
            addView(button(t("Kaydet", "Save")) {
                prefs.testSites = sites.text.lines().map(::normalizeRule).filter { it.isNotEmpty() }.distinct()
                sites.setText(prefs.testSites.joinToString("\n"))
                Toast.makeText(this@MainActivity, t("Kaydedildi", "Saved"), Toast.LENGTH_SHORT).show()
            })
        })

        root.addView(card(t("Güncelleme", "Updates")) {
            updText = text("", 14f, dim).apply { setPadding(0, dp(6), 0, dp(4)) }
            addView(updText)
            updButton = button("") {
                if (Updater.state.available != null) Updater.install(this@MainActivity) else Updater.check(this@MainActivity, manual = true)
            }
            addView(updButton)
        })

        root.addView(card(t("Hakkında", "About")) {
            val version = runCatching { packageManager.getPackageInfo(packageName, 0).versionName }.getOrNull() ?: ""
            addView(text("QuickDPI v$version", 16f, fg, true).apply { setPadding(0, dp(6), 0, 0) })
            addView(text(Texts.tagline, 14f, dim))
            addView(text(Texts.WEBSITE.removePrefix("https://"), 15f, accent, true).apply {
                setPadding(0, dp(8), 0, dp(8))
                setOnClickListener { runCatching { startActivity(Intent(Intent.ACTION_VIEW, android.net.Uri.parse(Texts.WEBSITE))) } }
            })
            addView(text(Texts.whyTitle, 15f, fg, true).apply { setPadding(0, dp(8), 0, dp(2)) })
            addView(text(Texts.why, 14f))
            addView(text(Texts.doesTitle, 15f, fg, true).apply { setPadding(0, dp(12), 0, dp(2)) })
            addView(bullets(Texts.does))
            addView(text(Texts.notTitle, 15f, fg, true).apply { setPadding(0, dp(12), 0, dp(2)) })
            addView(text(Texts.not, 14f))
            addView(text(Texts.license, 13f, dim).apply { setPadding(0, dp(12), 0, 0) })
            addView(button(t("Kullanım koşulları", "Terms of use")) { showTerms(false) })
        })

        rebuildRules()
        refresh()
        return ScrollView(this).apply {
            setBackgroundColor(bg)
            isFillViewport = true
            fitsSystemWindows = true
            addView(root)
        }
    }

    private fun refresh() {
        val on = QuickVpnService.running
        power.text = if (on) t("AÇIK", "ON") else t("KAPALI", "OFF")
        power.background = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(if (on) green else if (dark) Color.rgb(70, 74, 82) else Color.rgb(160, 165, 175))
        }
        status.text = when {
            on -> t("Koruma açık", "Protection on")
            QuickVpnService.lastError != null -> t("Başlatılamadı: ", "Could not start: ") + QuickVpnService.lastError
            else -> t("Koruma kapalı", "Protection off")
        }
        counters.text = if (on) t(
            "Bağlantı ${Stats.connections.get()} · korunan ${Stats.protected.get()} · hariç ${Stats.excluded.get()} · DNS ${Stats.dns.get()}",
            "Connections ${Stats.connections.get()} · protected ${Stats.protected.get()} · excluded ${Stats.excluded.get()} · DNS ${Stats.dns.get()}",
        ) else ""

        val s = prefs.activeStrategy
        strategyText.text = if (prefs.strategyChoice == "auto") t("Otomatik: ", "Automatic: ") + s.name else t("Elle: ", "Manual: ") + s.name
        advStrategy.text = if (prefs.strategyChoice == "auto") t("Otomatik", "Automatic") else s.name
        val st = AutoTune.state
        tuneButton.isEnabled = !st.running
        tuneText.text = when {
            st.running -> t("Deneniyor: ", "Testing: ") + st.step + st.results.joinToString("") { "\n${it.first}: ${it.second}/${it.third}" }
            st.message != null -> st.message
            prefs.tuned == null -> t("Henüz ayarlanmadı", "Not configured yet")
            else -> ""
        }
        tuneText.visibility = if (tuneText.text.isNullOrEmpty()) View.GONE else View.VISIBLE

        dnsButton.isEnabled = prefs.dnsEnabled
        dnsButton.text = when {
            prefs.dnsId == "custom" -> prefs.dns?.label ?: t("Özel (geçersiz)", "Custom (invalid)")
            else -> (DNS_PRESETS.firstOrNull { it.id == prefs.dnsId } ?: DNS_PRESETS[0]).label
        }

        val u = Updater.state
        val cur = Updater.current(this)
        updText.text = listOfNotNull(
            if (u.available != null) t("Yeni sürüm: ${u.available} (kurulu $cur)", "New version: ${u.available} (installed $cur)") else t("Kurulu sürüm: ", "Installed version: ") + cur,
            u.message,
        ).joinToString("\n")
        updButton.isEnabled = !u.busy
        updButton.text = if (u.available != null) t("Şimdi güncelle", "Update now") else t("Güncellemeleri denetle", "Check for updates")

        val recent = Stats.recent()
        if (recent != shownRecent) {
            shownRecent = recent
            rebuildRecent(recent)
        }
    }

    private fun changed() {
        QuickVpnService.reload(this)
        refresh()
    }

    private fun turnOn() {
        if (!prefs.accepted) { showTerms(true, true); return }
        val i = VpnService.prepare(this)
        if (i != null) {
            pendingStart = true
            @Suppress("DEPRECATION")
            startActivityForResult(i, 1)
        } else {
            Stats.reset()
            QuickVpnService.start(this)
        }
    }

    @Deprecated("Activity API")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == 1 && pendingStart) {
            pendingStart = false
            if (resultCode == RESULT_OK) {
                Stats.reset()
                QuickVpnService.start(this)
            } else {
                Toast.makeText(this, t("İzin verilmedi", "Permission denied"), Toast.LENGTH_SHORT).show()
            }
        }
    }

    private fun askNotify() {
        if (Build.VERSION.SDK_INT >= 33 && !prefs.askedNotify && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            prefs.askedNotify = true
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 2)
        }
    }

    private fun runTune() {
        AutoTune.start(prefs) { ui.post { changed() } }
        refresh()
    }

    private fun addRule(r: String) {
        if (r in prefs.rules) return
        prefs.rules = prefs.rules + r
        rebuildRules(); changed()
    }

    private fun removeRule(r: String) {
        prefs.rules = prefs.rules - r
        rebuildRules(); changed()
    }

    private fun copy(s: String) {
        getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("site", s))
        Toast.makeText(this, t("Kopyalandı", "Copied"), Toast.LENGTH_SHORT).show()
    }

    private fun rebuildRules() {
        rulesBox.removeAllViews()
        val rules = prefs.rules
        if (rules.isEmpty()) {
            rulesBox.addView(text(t("Kural yok", "No rules"), 13f, dim))
            return
        }
        for (r in rules) {
            val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL }
            val name = text(r).apply { layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f) }
            row.addView(name)
            row.addView(button(t("Kaldır", "Remove")) { removeRule(r) })
            row.setOnLongClickListener { v ->
                PopupMenu(this, v).apply {
                    menu.add(t("Kaldır", "Remove")).setOnMenuItemClickListener { removeRule(r); true }
                    menu.add(t("Kopyala", "Copy")).setOnMenuItemClickListener { copy(r); true }
                }.show()
                true
            }
            rulesBox.addView(row)
        }
    }

    private fun rebuildRecent(list: List<Pair<String, Boolean>>) {
        recentBox.removeAllViews()
        if (list.isEmpty()) {
            recentBox.addView(text(t("Henüz yok", "None yet"), 13f, dim))
            return
        }
        for ((host, applied) in list.take(20)) {
            val v = text((if (applied) "● " else "○ ") + host, 14f, if (applied) fg else dim).apply {
                setPadding(0, dp(6), 0, dp(6))
                setOnClickListener { recentMenu(this, host) }
            }
            recentBox.addView(v)
        }
    }

    private fun recentMenu(anchor: View, host: String) {
        val rule = prefs.rules.firstOrNull { ruleMatches(it, host) }
        PopupMenu(this, anchor).apply {
            if (rule != null) {
                menu.add(t("Kurallardan çıkar: ", "Remove from rules: ") + rule).setOnMenuItemClickListener { removeRule(rule); true }
            } else {
                menu.add(t("Kurallara ekle: ", "Add to rules: ") + host).setOnMenuItemClickListener { addRule(host); true }
                val base = host.split('.').takeLast(2).joinToString(".")
                if (base != host) menu.add(t("Kurallara ekle: ", "Add to rules: ") + base).setOnMenuItemClickListener { addRule(base); true }
            }
            menu.add(t("Kopyala", "Copy")).setOnMenuItemClickListener { copy(host); true }
        }.show()
    }

    private fun strategyDialog() {
        val names = listOf(t("Otomatik (önerilen)", "Automatic (recommended)")) + STRATEGIES.map { it.name }
        val ids = listOf("auto") + STRATEGIES.map { it.id }
        AlertDialog.Builder(this)
            .setTitle(t("Yöntem", "Method"))
            .setSingleChoiceItems(names.toTypedArray(), ids.indexOf(prefs.strategyChoice).coerceAtLeast(0)) { d, i ->
                prefs.strategyChoice = ids[i]
                d.dismiss(); changed()
            }
            .show()
    }

    private sealed class DnsRow {
        class Header(val title: String) : DnsRow()
        class Item(val preset: DnsPreset, val custom: String? = null) : DnsRow()
    }

    private fun dnsDialog() {
        val box = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(dp(20), dp(8), dp(20), 0) }
        val search = EditText(this).apply {
            hint = t("Ara ya da IP:port yazın", "Search or type IP:port")
            isSingleLine = true
            if (prefs.dnsId == "custom") setText(prefs.dnsCustom)
        }
        box.addView(search)
        val list = ListView(this).apply { layoutParams = LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(380)) }
        box.addView(list)
        var rows: List<DnsRow> = emptyList()
        val adapter = object : ArrayAdapter<String>(this, android.R.layout.simple_list_item_1) {
            override fun isEnabled(position: Int) = rows[position] is DnsRow.Item
            override fun getView(position: Int, convertView: View?, parent: ViewGroup): View {
                val v = super.getView(position, convertView, parent) as TextView
                when (val r = rows[position]) {
                    is DnsRow.Header -> { v.typeface = Typeface.DEFAULT_BOLD; v.textSize = 13f; v.alpha = 0.7f }
                    is DnsRow.Item -> {
                        v.typeface = if (r.custom == null && r.preset.id == prefs.dnsId) Typeface.DEFAULT_BOLD else Typeface.DEFAULT
                        v.textSize = 15f; v.alpha = 1f
                    }
                }
                return v
            }
        }
        list.adapter = adapter
        fun fill(q: String) {
            val k = q.trim().lowercase()
            val out = ArrayList<DnsRow>()
            parseCustomDns(q)?.let { out += DnsRow.Item(it, q.trim()) }
            val hits = DNS_PRESETS.filter { k.isEmpty() || it.label.lowercase().contains(k) }
            val alt = hits.filter { it.altPort }
            val std = hits.filter { !it.altPort }
            if (alt.isNotEmpty()) { out += DnsRow.Header(t("Özel port (önerilen)", "Custom port (recommended)")); alt.forEach { out += DnsRow.Item(it) } }
            if (std.isNotEmpty()) { out += DnsRow.Header(t("Standart port 53", "Standard port 53")); std.forEach { out += DnsRow.Item(it) } }
            rows = out
            adapter.clear()
            adapter.addAll(out.map { r -> when (r) { is DnsRow.Header -> r.title; is DnsRow.Item -> if (r.custom != null) t("Özel: ", "Custom: ") + r.preset.label else r.preset.label } })
        }
        fill("")
        search.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
            override fun onTextChanged(s: CharSequence?, a: Int, b: Int, c: Int) {}
            override fun afterTextChanged(s: Editable?) = fill(s.toString())
        })
        val dlg = AlertDialog.Builder(this)
            .setTitle(t("DNS sunucusu", "DNS server"))
            .setView(box)
            .setNegativeButton(t("Vazgeç", "Cancel"), null)
            .create()
        list.setOnItemClickListener { _, _, pos, _ ->
            val r = rows[pos] as? DnsRow.Item ?: return@setOnItemClickListener
            if (r.custom != null) {
                prefs.dnsId = "custom"; prefs.dnsCustom = r.custom
            } else {
                prefs.dnsId = r.preset.id
            }
            dlg.dismiss(); changed()
        }
        dlg.show()
    }
}
