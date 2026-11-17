package dev.earthquick.quickdpi

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ProxyInfo
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.service.quicksettings.TileService
import android.content.ComponentName
import java.io.FileInputStream
import java.io.FileOutputStream
import java.util.concurrent.Executors

class QuickVpnService : VpnService() {
    companion object {
        const val ACTION_START = "dev.earthquick.quickdpi.START"
        const val ACTION_STOP = "dev.earthquick.quickdpi.STOP"
        const val ACTION_RELOAD = "dev.earthquick.quickdpi.RELOAD"
        private const val DNS4 = "10.111.222.1"
        private const val ADDR4 = "10.111.222.2"
        private const val DNS6 = "fd00:111:222::1"
        private const val ADDR6 = "fd00:111:222::2"
        private const val CHANNEL = "status"

        @Volatile var running = false
            private set
        @Volatile var lastError: String? = null

        fun start(ctx: Context) {
            ctx.startForegroundService(Intent(ctx, QuickVpnService::class.java).setAction(ACTION_START))
        }

        fun stop(ctx: Context) {
            ctx.startService(Intent(ctx, QuickVpnService::class.java).setAction(ACTION_STOP))
        }

        fun reload(ctx: Context) {
            if (running) ctx.startService(Intent(ctx, QuickVpnService::class.java).setAction(ACTION_RELOAD))
        }
    }

    private var tun: ParcelFileDescriptor? = null
    private var proxy: LocalProxy? = null
    private var reader: Thread? = null
    private val dnsPool = Executors.newFixedThreadPool(8)
    @Volatile private var dnsClient: DnsClient? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                Prefs(this).enabled = false
                shutdown()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_RELOAD -> if (running) establish()
            else -> {
                goForeground()
                if (!Prefs(this).accepted) {
                    stopForeground(STOP_FOREGROUND_REMOVE)
                    stopSelf()
                    return START_NOT_STICKY
                }
                Prefs(this).enabled = true
                establish()
            }
        }
        return START_STICKY
    }

    override fun onRevoke() {
        Prefs(this).enabled = false
        shutdown()
        stopSelf()
    }

    override fun onDestroy() {
        shutdown()
        dnsPool.shutdownNow()
        super.onDestroy()
    }

    private fun goForeground() {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, t("Koruma durumu", "Protection status"), NotificationManager.IMPORTANCE_LOW))
        startForeground(1, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
    }

    private fun notification(): Notification {
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val stop = PendingIntent.getService(this, 1, Intent(this, QuickVpnService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)
        val p = Prefs(this)
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(t("Koruma açık", "Protection on"))
            .setContentText(p.activeStrategy.name + (p.dns?.let { " · " + it.name } ?: ""))
            .setContentIntent(open)
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null, t("Kapat", "Turn off"), stop).build())
            .build()
    }

    @Synchronized
    private fun establish() {
        val p = Prefs(this)
        try {
            closeTun()
            val dns = p.dns
            dnsClient = dns?.let { DnsClient(it) }
            if (proxy == null) {
                proxy = LocalProxy({ Prefs(this).activeStrategy }, { Prefs(this).ruleMode }, { Prefs(this).rules }, { dnsClient }).start()
            }
            val b = Builder()
                .setSession("QuickDPI")
                .setConfigureIntent(PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE))
                .addAddress(ADDR4, 32)
                .addRoute(DNS4, 32)
                .addAddress(ADDR6, 128)
                .addRoute(DNS6, 128)
                .setMtu(1500)
                .setBlocking(true)
                .setMetered(false)
            if (dns != null) {
                b.addDnsServer(DNS4)
                b.addDnsServer(DNS6)
            }
            val excl = if (p.ruleMode == RuleMode.EXCEPT) p.rules.flatMap { listOf(it, "*.$it") } else emptyList()
            b.setHttpProxy(ProxyInfo.buildDirectProxy("127.0.0.1", proxy!!.port, excl))
            b.addDisallowedApplication(packageName)
            val fd = b.establish() ?: throw IllegalStateException(t("VPN izni yok", "No VPN permission"))
            tun = fd
            running = true
            lastError = null
            reader = Thread({ readLoop(fd) }, "qd-tun").apply { start() }
            getSystemService(NotificationManager::class.java).notify(1, notification())
        } catch (e: Exception) {
            lastError = e.message ?: e.toString()
            shutdown()
        }
        refreshTile()
    }

    private fun readLoop(fd: ParcelFileDescriptor) {
        val inp = FileInputStream(fd.fileDescriptor)
        val out = FileOutputStream(fd.fileDescriptor)
        val buf = ByteArray(32767)
        while (true) {
            val n = try {
                inp.read(buf)
            } catch (e: Exception) {
                break
            }
            if (n <= 0) {
                if (n < 0) break else continue
            }
            val pkt = buf.copyOf(n)
            val q = Packets.dnsQuery(pkt) ?: continue
            dnsPool.execute {
                val client = dnsClient ?: return@execute
                val resp = client.forward(q.payload) ?: return@execute
                Stats.dns.incrementAndGet()
                val reply = Packets.reply(q, resp)
                synchronized(out) { runCatching { out.write(reply) } }
            }
        }
    }

    private fun closeTun() {
        running = false
        runCatching { tun?.close() }
        tun = null
        reader?.join(1000)
        reader = null
    }

    @Synchronized
    private fun shutdown() {
        closeTun()
        proxy?.stop()
        proxy = null
        refreshTile()
    }

    private fun refreshTile() {
        runCatching { TileService.requestListeningState(this, ComponentName(this, ToggleTile::class.java)) }
    }
}
