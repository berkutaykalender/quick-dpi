package dev.earthquick.quickdpi

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

object Updater {
    private const val APK = "quickdpi-android.apk"
    private const val EVERY_MS = 6 * 3600 * 1000L

    class State(
        val available: String? = null,
        val busy: Boolean = false,
        val message: String? = null,
    )

    @Volatile var state = State()
        private set
    @Volatile private var sha: String? = null
    @Volatile private var lastCheck = 0L

    fun current(ctx: Context): String =
        runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName }.getOrNull() ?: "0.0.0"

    private fun parse(v: String): List<Int>? =
        v.trim().removePrefix("v").split('.').map { it.toIntOrNull() ?: return null }.takeIf { it.size == 3 }

    fun newer(latest: String, current: String): Boolean {
        val a = parse(latest) ?: return false
        val b = parse(current) ?: return false
        for (i in 0..2) if (a[i] != b[i]) return a[i] > b[i]
        return false
    }

    private fun open(url: String, timeout: Int) = (URL(url).openConnection() as HttpURLConnection).apply {
        connectTimeout = 15_000
        readTimeout = timeout
        setRequestProperty("User-Agent", "QuickDPI-Android")
    }

    fun checkIfDue(ctx: Context) {
        if (System.currentTimeMillis() - lastCheck > EVERY_MS) check(ctx, manual = false)
    }

    fun check(ctx: Context, manual: Boolean) {
        if (state.busy) return
        val app = ctx.applicationContext
        lastCheck = System.currentTimeMillis()
        state = State(state.available, busy = true)
        Thread {
            state = try {
                val js = JSONObject(open("${Texts.WEBSITE}/downloads/latest.json", 20_000).inputStream.bufferedReader().use { it.readText() })
                val v = js.getString("version")
                val files = js.getJSONArray("files")
                sha = (0 until files.length()).map { files.getJSONObject(it) }.firstOrNull { it.getString("name") == APK }?.getString("sha256")
                if (newer(v, current(app)) && sha != null) State(available = v)
                else State(message = if (manual) t("QuickDPI güncel.", "QuickDPI is up to date.") else null)
            } catch (e: Exception) {
                State(state.available, message = if (manual) t("Güncelleme denetlenemedi: ", "Could not check for updates: ") + e.message else null)
            }
        }.start()
    }

    fun install(ctx: Context) {
        val v = state.available ?: return
        val want = sha ?: return
        if (state.busy) return
        val app = ctx.applicationContext
        state = State(v, busy = true, message = t("Sürüm indiriliyor: ", "Downloading version ") + v)
        Thread {
            state = try {
                val dir = File(app.cacheDir, "update").apply { mkdirs() }
                val file = File(dir, APK)
                val md = MessageDigest.getInstance("SHA-256")
                open("${Texts.WEBSITE}/downloads/$APK", 120_000).inputStream.use { inp ->
                    file.outputStream().use { out ->
                        val buf = ByteArray(64 * 1024)
                        while (true) {
                            val n = inp.read(buf)
                            if (n < 0) break
                            md.update(buf, 0, n)
                            out.write(buf, 0, n)
                        }
                    }
                }
                val got = md.digest().joinToString("") { "%02x".format(it) }
                if (!got.equals(want.trim(), ignoreCase = true)) {
                    file.delete()
                    error(t("sha256 tutmadı", "sha256 mismatch"))
                }
                commit(app, file)
                State(v, message = t("Kurulum onayı bekleniyor…", "Waiting for install confirmation…"))
            } catch (e: Exception) {
                State(v, message = t("Güncelleme kurulamadı: ", "Update failed: ") + e.message)
            }
        }.start()
    }

    private fun commit(ctx: Context, apk: File) {
        val pi = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(ctx.packageName)
            if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = pi.createSession(params)
        pi.openSession(id).use { s ->
            s.openWrite("quickdpi.apk", 0, apk.length()).use { out -> apk.inputStream().use { it.copyTo(out) }; s.fsync(out) }
            val intent = Intent(ctx, UpdateReceiver::class.java)
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or (if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0)
            s.commit(PendingIntent.getBroadcast(ctx, id, intent, flags).intentSender)
        }
    }

    internal fun finished(msg: String?) {
        state = State(state.available, message = msg)
    }
}

class UpdateReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        when (intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirm = intent.getParcelableExtra<Intent>(Intent.EXTRA_INTENT) ?: return
                runCatching { ctx.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            }
            PackageInstaller.STATUS_SUCCESS -> Updater.finished(null)
            else -> Updater.finished(
                t("Güncelleme kurulamadı: ", "Update failed: ") + (intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE) ?: "")
            )
        }
    }
}
