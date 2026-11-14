package io.github.codyps.echolocate.recorder

import android.hardware.GeomagneticField
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import org.json.JSONObject
import kotlin.math.asin
import kotlin.math.atan2
import kotlin.math.sqrt

/**
 * Barometer (1 Hz means) and device pointing for labels. Sensor timestamps share the
 * elapsedRealtimeNanos base used by every other record.
 */
class SensorCapture(private val sm: SensorManager, private val store: SessionStore, private val handler: Handler) :
    SensorEventListener {
    private val rotation = FloatArray(9)
    @Volatile private var haveRotation = false
    private var pSum = 0.0
    private var pN = 0
    private var pStart = 0L
    @Volatile var lastPressureHpa: Double? = null
        private set

    fun start() {
        sm.getDefaultSensor(Sensor.TYPE_PRESSURE)?.let { sm.registerListener(this, it, 200_000, handler) }
        sm.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR)?.let { sm.registerListener(this, it, 100_000, handler) }
    }

    fun stop() = sm.unregisterListener(this)

    override fun onSensorChanged(e: SensorEvent) {
        when (e.sensor.type) {
            Sensor.TYPE_PRESSURE -> {
                if (pN == 0) pStart = e.timestamp
                pSum += e.values[0]; pN++
                if (e.timestamp - pStart >= 1_000_000_000L) {
                    val hpa = pSum / pN
                    lastPressureHpa = hpa
                    store.sensors.json(
                        JSONObject().put("type", "pressure").put("mono_ns", (pStart + e.timestamp) / 2)
                            .put("hpa", hpa).put("samples", pN)
                    )
                    pSum = 0.0; pN = 0
                }
            }
            Sensor.TYPE_ROTATION_VECTOR -> synchronized(rotation) {
                SensorManager.getRotationMatrixFromVector(rotation, e.values)
                haveRotation = true
            }
        }
    }

    override fun onAccuracyChanged(sensor: Sensor, accuracy: Int) {}

    /**
     * Direction the top edge of the phone points: (azimuth from true north, elevation), degrees.
     * Uses the device +Y axis in world (east, north, up) coordinates, which stays well defined
     * when the phone is tilted up toward the sky, unlike SensorManager.getOrientation.
     */
    fun pointing(lat: Double?, lon: Double?, heightM: Double?): Pair<Double, Double>? {
        if (!haveRotation) return null
        val (e, n, u) = synchronized(rotation) { Triple(rotation[1], rotation[4], rotation[7]) }
        val norm = sqrt((e * e + n * n + u * u).toDouble())
        var az = Math.toDegrees(atan2(e.toDouble(), n.toDouble()))
        if (lat != null && lon != null) {
            az += GeomagneticField(lat.toFloat(), lon.toFloat(), (heightM ?: 0.0).toFloat(), System.currentTimeMillis()).declination
        }
        az = ((az % 360) + 360) % 360
        return az to Math.toDegrees(asin((u / norm).coerceIn(-1.0, 1.0)))
    }
}
