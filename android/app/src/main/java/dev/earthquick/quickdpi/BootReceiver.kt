package dev.earthquick.quickdpi

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.net.VpnService

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (Prefs(ctx).let { it.enabled && it.accepted } && VpnService.prepare(ctx) == null) {
            runCatching { QuickVpnService.start(ctx) }
        }
    }
}
