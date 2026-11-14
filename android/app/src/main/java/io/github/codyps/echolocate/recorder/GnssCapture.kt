package io.github.codyps.echolocate.recorder

import android.annotation.SuppressLint
import android.location.GnssClock
import android.location.GnssMeasurement
import android.location.GnssMeasurementRequest
import android.location.GnssMeasurementsEvent
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.location.LocationRequest
import android.os.Build
import android.os.SystemClock
import org.json.JSONObject
import java.util.concurrent.Executor

/** GnssLogger-compatible "Raw" rows, so existing converters can produce RINEX for PPK/PPP. */
object GnssRaw {
    private val columns = listOf(
        "utcTimeMillis", "TimeNanos", "LeapSecond", "TimeUncertaintyNanos", "FullBiasNanos",
        "BiasNanos", "BiasUncertaintyNanos", "DriftNanosPerSecond", "DriftUncertaintyNanosPerSecond",
        "HardwareClockDiscontinuityCount", "Svid", "TimeOffsetNanos", "State", "ReceivedSvTimeNanos",
        "ReceivedSvTimeUncertaintyNanos", "Cn0DbHz", "PseudorangeRateMetersPerSecond",
        "PseudorangeRateUncertaintyMetersPerSecond", "AccumulatedDeltaRangeState",
        "AccumulatedDeltaRangeMeters", "AccumulatedDeltaRangeUncertaintyMeters", "CarrierFrequencyHz",
        "CarrierCycles", "CarrierPhase", "CarrierPhaseUncertainty", "MultipathIndicator", "SnrInDb",
        "ConstellationType", "AgcDb", "BasebandCn0DbHz", "FullInterSignalBiasNanos",
        "FullInterSignalBiasUncertaintyNanos", "SatelliteInterSignalBiasNanos",
        "SatelliteInterSignalBiasUncertaintyNanos", "CodeType", "ChipsetElapsedRealtimeNanos",
    )

    fun header(recorder: JSONObject): String =
        "# Version: v3.0.0.0 Platform: ${recorder.optString("os_version")} " +
            "Manufacturer: ${recorder.optString("manufacturer")} Model: ${recorder.optString("model")}\n" +
            "# Written by echolocate-recorder; columns follow GnssLogger's Raw record.\n" +
            "# Raw," + columns.joinToString(",") + "\n"

    private fun f(has: Boolean, v: () -> Any): String = if (has) v().toString() else ""

    fun row(c: GnssClock, m: GnssMeasurement): String {
        val utcMs = if (c.hasFullBiasNanos()) {
            val gpsNs = c.timeNanos - c.fullBiasNanos - (if (c.hasBiasNanos()) c.biasNanos.toLong() else 0L)
            val leap = if (c.hasLeapSecond()) c.leapSecond else 18
            gpsNs / 1_000_000 + (315_964_800L - leap) * 1000
        } else null
        val fields = listOf(
            utcMs?.toString() ?: "",
            c.timeNanos.toString(),
            f(c.hasLeapSecond()) { c.leapSecond },
            f(c.hasTimeUncertaintyNanos()) { c.timeUncertaintyNanos },
            f(c.hasFullBiasNanos()) { c.fullBiasNanos },
            f(c.hasBiasNanos()) { c.biasNanos },
            f(c.hasBiasUncertaintyNanos()) { c.biasUncertaintyNanos },
            f(c.hasDriftNanosPerSecond()) { c.driftNanosPerSecond },
            f(c.hasDriftUncertaintyNanosPerSecond()) { c.driftUncertaintyNanosPerSecond },
            c.hardwareClockDiscontinuityCount.toString(),
            m.svid.toString(),
            m.timeOffsetNanos.toString(),
            m.state.toString(),
            m.receivedSvTimeNanos.toString(),
            m.receivedSvTimeUncertaintyNanos.toString(),
            m.cn0DbHz.toString(),
            m.pseudorangeRateMetersPerSecond.toString(),
            m.pseudorangeRateUncertaintyMetersPerSecond.toString(),
            m.accumulatedDeltaRangeState.toString(),
            m.accumulatedDeltaRangeMeters.toString(),
            m.accumulatedDeltaRangeUncertaintyMeters.toString(),
            f(m.hasCarrierFrequencyHz()) { m.carrierFrequencyHz },
            "", "", "", // carrier cycles/phase: deprecated, never populated
            m.multipathIndicator.toString(),
            f(m.hasSnrInDb()) { m.snrInDb },
            m.constellationType.toString(),
            f(m.hasAutomaticGainControlLevelDb()) { m.automaticGainControlLevelDb },
            f(m.hasBasebandCn0DbHz()) { m.basebandCn0DbHz },
            f(m.hasFullInterSignalBiasNanos()) { m.fullInterSignalBiasNanos },
            f(m.hasFullInterSignalBiasUncertaintyNanos()) { m.fullInterSignalBiasUncertaintyNanos },
            f(m.hasSatelliteInterSignalBiasNanos()) { m.satelliteInterSignalBiasNanos },
            f(m.hasSatelliteInterSignalBiasUncertaintyNanos()) { m.satelliteInterSignalBiasUncertaintyNanos },
            f(m.hasCodeType()) { m.codeType },
            f(c.hasElapsedRealtimeNanos()) { c.elapsedRealtimeNanos },
        )
        return "Raw," + fields.joinToString(",")
    }
}

/**
 * GPS-provider fixes (ellipsoidal height), GnssClock time anchors, and raw measurements.
 * Uses the GPS provider directly, not the fused provider, so every fix is a GNSS solution.
 */
class GnssCapture(
    private val lm: LocationManager,
    private val store: SessionStore,
    private val executor: Executor,
) {
    @Volatile var lastFix: Location? = null
        private set
    @Volatile var gnssAnchors = 0L
        private set
    @Volatile var measurementsSeen = 0L
        private set

    private val locationListener = LocationListener { l -> onLocation(l) }
    private val measurementCallback = object : GnssMeasurementsEvent.Callback() {
        override fun onGnssMeasurementsReceived(e: GnssMeasurementsEvent) = onMeasurements(e)
    }

    @SuppressLint("MissingPermission") // Checked by MainActivity before the service starts.
    fun start() {
        val req = LocationRequest.Builder(1000L)
            .setQuality(LocationRequest.QUALITY_HIGH_ACCURACY)
            .setMinUpdateIntervalMillis(1000L)
            .build()
        lm.requestLocationUpdates(LocationManager.GPS_PROVIDER, req, executor, locationListener)
        // Full tracking disables duty cycling so carrier phase stays continuous for PPK.
        val mreq = GnssMeasurementRequest.Builder().setFullTracking(true).build()
        lm.registerGnssMeasurementsCallback(mreq, executor, measurementCallback)
    }

    fun stop() {
        lm.removeUpdates(locationListener)
        lm.unregisterGnssMeasurementsCallback(measurementCallback)
    }

    private fun onLocation(l: Location) {
        lastFix = l
        val o = JSONObject()
            .put("mono_ns", l.elapsedRealtimeNanos)
            .put("unix_ms", l.time)
            .put("provider", l.provider ?: "")
            .put("lat", l.latitude)
            .put("lon", l.longitude)
        // Location.getAltitude() is height above the WGS84 ellipsoid.
        if (l.hasAltitude()) o.put("h_ellipsoid_m", l.altitude)
        if (Build.VERSION.SDK_INT >= 34 && l.hasMslAltitude()) o.put("h_msl_m", l.mslAltitudeMeters)
        if (l.hasAccuracy()) o.put("h_acc_m", l.accuracy.toDouble())
        if (l.hasVerticalAccuracy()) o.put("v_acc_m", l.verticalAccuracyMeters.toDouble())
        if (l.hasSpeed()) o.put("speed_m_s", l.speed.toDouble())
        if (l.hasBearing()) o.put("bearing_deg", l.bearing.toDouble())
        if (l.hasElapsedRealtimeUncertaintyNanos()) o.put("mono_unc_ns", l.elapsedRealtimeUncertaintyNanos)
        store.location.json(o)
    }

    private fun onMeasurements(e: GnssMeasurementsEvent) {
        val c = e.clock
        for (m in e.measurements) store.gnssRaw.line(GnssRaw.row(c, m))
        measurementsSeen += e.measurements.size
        // Only a receiver with a time solution (full bias known) defines GPS time.
        if (c.hasFullBiasNanos() && c.hasElapsedRealtimeNanos()) {
            val o = JSONObject()
                .put("type", "gnss")
                .put("mono_ns", c.elapsedRealtimeNanos)
                .put("time_nanos", c.timeNanos)
                .put("full_bias_nanos", c.fullBiasNanos)
                .put("bias_nanos", if (c.hasBiasNanos()) c.biasNanos else 0.0)
                .put("hw_discontinuity", c.hardwareClockDiscontinuityCount)
            if (c.hasElapsedRealtimeUncertaintyNanos()) o.put("mono_unc_ns", c.elapsedRealtimeUncertaintyNanos)
            if (c.hasBiasUncertaintyNanos()) o.put("bias_unc_ns", c.biasUncertaintyNanos)
            if (c.hasLeapSecond()) o.put("leap_s", c.leapSecond)
            store.clock.json(o)
            gnssAnchors++
        }
    }

    companion object {
        /** Wall-clock anchor: the system clock read between two boot-clock reads. */
        fun wallAnchor(store: SessionStore) {
            val a = SystemClock.elapsedRealtimeNanos()
            val w = System.currentTimeMillis()
            val b = SystemClock.elapsedRealtimeNanos()
            store.clock.json(
                JSONObject().put("type", "wall").put("mono_ns", a + (b - a) / 2)
                    .put("unix_ns", w * 1_000_000L).put("half_gap_ns", (b - a) / 2)
            )
        }
    }
}
