// SPDX-License-Identifier: Apache-2.0
package app.openworld.platform

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.BatteryManager
import android.os.Build
import android.os.PowerManager
import androidx.core.content.ContextCompat
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import app.openworld.core.Stop

/** Heat from PowerManager, battery from the sticky broadcast, foreground from the process lifecycle. */
class AndroidDevice(context: Context) : Device {
    private val app = context.applicationContext
    private val power = app.getSystemService(PowerManager::class.java)

    override fun status(): DeviceStatus {
        val battery = batteryOf(ContextCompat.registerReceiver(app, null, IntentFilter(Intent.ACTION_BATTERY_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED))
        val thermal = if (Build.VERSION.SDK_INT >= 29 && power != null) thermalName(power.currentThermalStatus) else null
        return DeviceStatus(thermal, battery?.first, battery?.second)
    }

    override fun watch(onStop: (String) -> Unit): AutoCloseable {
        val notify = onStop
        val closers = mutableListOf<() -> Unit>()

        if (Build.VERSION.SDK_INT >= 29 && power != null) {
            val listener = PowerManager.OnThermalStatusChangedListener { status ->
                if (status >= PowerManager.THERMAL_STATUS_SEVERE) notify(Stop.HEAT)
            }
            power.addThermalStatusListener(ContextCompat.getMainExecutor(app), listener)
            closers += { power.removeThermalStatusListener(listener) }
        }

        val receiver = object : BroadcastReceiver() {
            override fun onReceive(context: Context, intent: Intent) {
                val (level, charging) = batteryOf(intent) ?: return
                if (level < LOW_BATTERY && !charging) notify(Stop.BATTERY)
            }
        }
        // The current state arrives at once: the battery broadcast is sticky.
        ContextCompat.registerReceiver(app, receiver, IntentFilter(Intent.ACTION_BATTERY_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED)
        closers += { runCatching { app.unregisterReceiver(receiver) } }

        val lifecycle = ProcessLifecycleOwner.get().lifecycle
        val observer = object : DefaultLifecycleObserver {
            override fun onStop(owner: LifecycleOwner) = notify(Stop.BACKGROUND)
        }
        lifecycle.addObserver(observer)
        closers += { lifecycle.removeObserver(observer) }

        return AutoCloseable { closers.forEach { it() } }
    }

    /** Level from 0 to 1, and whether it is charging. */
    private fun batteryOf(intent: Intent?): Pair<Float, Boolean>? {
        if (intent == null) return null
        val level = intent.getIntExtra(BatteryManager.EXTRA_LEVEL, -1)
        val scale = intent.getIntExtra(BatteryManager.EXTRA_SCALE, -1)
        if (level < 0 || scale <= 0) return null
        val status = intent.getIntExtra(BatteryManager.EXTRA_STATUS, -1)
        val plugged = intent.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0)
        val charging = plugged != 0 || status == BatteryManager.BATTERY_STATUS_CHARGING || status == BatteryManager.BATTERY_STATUS_FULL
        return Pair(level.toFloat() / scale, charging)
    }

    /**
     * The core's words for heat, as Apple's thermal states use them: nominal, fair, serious, critical.
     * The scan stops at serious ([PowerManager.THERMAL_STATUS_SEVERE]) and above.
     */
    private fun thermalName(status: Int): String = when {
        status <= PowerManager.THERMAL_STATUS_NONE -> "nominal"
        status <= PowerManager.THERMAL_STATUS_MODERATE -> "fair"
        status == PowerManager.THERMAL_STATUS_SEVERE -> "serious"
        else -> "critical"
    }

    private companion object {
        const val LOW_BATTERY = 0.10f
    }
}
