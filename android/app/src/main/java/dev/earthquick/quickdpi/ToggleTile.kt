package dev.earthquick.quickdpi

import android.app.PendingIntent
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

class ToggleTile : TileService() {
    override fun onStartListening() = update()

    override fun onClick() {
        if (QuickVpnService.running) {
            QuickVpnService.stop(this)
        } else if (Prefs(this).accepted && VpnService.prepare(this) == null) {
            QuickVpnService.start(this)
        } else {
            val i = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("start", true)
            if (Build.VERSION.SDK_INT >= 34) {
                startActivityAndCollapse(PendingIntent.getActivity(this, 2, i, PendingIntent.FLAG_IMMUTABLE))
            } else {
                @Suppress("DEPRECATION")
                startActivityAndCollapse(i)
            }
        }
        qsTile?.let {
            it.state = if (QuickVpnService.running) Tile.STATE_INACTIVE else Tile.STATE_ACTIVE
            it.updateTile()
        }
    }

    private fun update() {
        val tile = qsTile ?: return
        tile.state = if (QuickVpnService.running) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "QuickDPI"
        if (Build.VERSION.SDK_INT >= 29) tile.subtitle = if (QuickVpnService.running) t("Açık", "On") else t("Kapalı", "Off")
        tile.updateTile()
    }
}
