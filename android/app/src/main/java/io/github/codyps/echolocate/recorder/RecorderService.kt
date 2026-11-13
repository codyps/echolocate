package io.github.codyps.echolocate.recorder

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.hardware.SensorManager
import android.location.LocationManager
import android.media.AudioManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.os.PowerManager
import android.os.SystemClock
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONObject
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

data class Status(
    val recording: Boolean = false,
    val sessionId: String? = null,
    val elapsedS: Long = 0,
    val segments: Int = 0,
    val unprocessed: Boolean? = null,
    val peakDbfs: Double = -120.0,
    val rmsDbfs: Double = -120.0,
    val clipped: Long = 0,
    val lat: Double? = null,
    val lon: Double? = null,
    val hAccM: Double? = null,
    val heightM: Double? = null,
    val gnssAnchors: Long = 0,
    val rawMeasurements: Long = 0,
    val pressureHpa: Double? = null,
    val labels: Int = 0,
    val lastLabel: String? = null,
    val error: String? = null,
)

/** Process-wide handle to the active session, shared by the service and the UI. */
object Recorder {
    internal val mutable = MutableStateFlow(Status())
    val status: StateFlow<Status> = mutable
    @Volatile internal var active: RecorderService? = null

    fun sessionsDir(ctx: Context): File = File(ctx.getExternalFilesDir(null), "sessions").apply { mkdirs() }

    fun label(kind: String, text: String = ""): Boolean = active?.label(kind, text) ?: false
}

/**
 * Foreground service (microphone + location) that owns one recording session.
 * Start with [ACTION_START] and extras; stop with [ACTION_STOP].
 */
class RecorderService : Service() {
    private lateinit var thread: HandlerThread
    private lateinit var handler: Handler
    private var store: SessionStore? = null
    private var audio: AudioCapture? = null
    private var gnss: GnssCapture? = null
    private var sensors: SensorCapture? = null
    private var wakeLock: PowerManager.WakeLock? = null
    private var startedAt = 0L
    private var labelCount = 0
    private var lastLabel: String? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_START -> if (store == null) start(intent)
            ACTION_STOP -> stop()
        }
        return START_NOT_STICKY
    }

    private fun start(intent: Intent) {
        startForeground(
            NOTIFICATION_ID, notification("Starting…"),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE or ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION
        )
        thread = HandlerThread("echolocate-sensors").also { it.start() }
        handler = Handler(thread.looper)
        try {
            val rate = intent.getIntExtra(EXTRA_RATE, 48_000)
            val channels = intent.getIntExtra(EXTRA_CHANNELS, 1)
            val micHeight = intent.getDoubleExtra(EXTRA_MIC_HEIGHT, Double.NaN).takeIf { !it.isNaN() }
            val notes = intent.getStringExtra(EXTRA_NOTES) ?: ""
            val created = System.currentTimeMillis()
            val id = SimpleDateFormat("yyyyMMdd'T'HHmmss'Z'", Locale.US)
                .apply { timeZone = TimeZone.getTimeZone("UTC") }.format(Date(created)) + "-" + Build.DEVICE
            val a = AudioCapture(getSystemService(AudioManager::class.java), rate, channels)
            audio = a
            val audioInfo = a.prepare()
            val s = SessionStore(
                File(Recorder.sessionsDir(this), id), id, created, recorderInfo(), audioInfo,
                audioInfo.getInt("sample_rate_hz"), channels, micHeight, notes,
            )
            store = s
            gnss = GnssCapture(getSystemService(LocationManager::class.java), s) { handler.post(it) }.also { it.start() }
            sensors = SensorCapture(getSystemService(SensorManager::class.java), s, handler).also { it.start() }
            GnssCapture.wallAnchor(s)
            a.start(s)
            wakeLock = getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "echolocate:recording").apply { acquire() }
            startedAt = SystemClock.elapsedRealtime()
            Recorder.active = this
            handler.post(tick)
        } catch (t: Throwable) {
            Recorder.mutable.value = Status(error = "Could not start: ${t.message}")
            stop()
        }
    }

    private var ticks = 0L
    private val tick: Runnable = object : Runnable {
        override fun run() {
            val s = store ?: return
            ticks++
            if (ticks % 20 == 0L) GnssCapture.wallAnchor(s) // every 10 s
            if (ticks % 4 == 0L) s.flushLogs()
            publish()
            if (ticks % 2 == 0L) {
                getSystemService(NotificationManager::class.java)
                    .notify(NOTIFICATION_ID, notification(summary()))
            }
            if (audio?.error != null) stop() else handler.postDelayed(this, 500)
        }
    }

    private fun publish() {
        val s = store ?: return
        val a = audio
        val fix = gnss?.lastFix
        Recorder.mutable.value = Status(
            recording = true,
            sessionId = s.sessionId,
            elapsedS = (SystemClock.elapsedRealtime() - startedAt) / 1000,
            segments = s.segmentCount,
            unprocessed = a?.unprocessedSupported,
            peakDbfs = a?.peakDbfs ?: -120.0,
            rmsDbfs = a?.rmsDbfs ?: -120.0,
            clipped = a?.clippedSamples ?: 0,
            lat = fix?.latitude,
            lon = fix?.longitude,
            hAccM = fix?.takeIf { it.hasAccuracy() }?.accuracy?.toDouble(),
            heightM = fix?.takeIf { it.hasAltitude() }?.altitude,
            gnssAnchors = gnss?.gnssAnchors ?: 0,
            rawMeasurements = gnss?.measurementsSeen ?: 0,
            pressureHpa = sensors?.lastPressureHpa,
            labels = labelCount,
            lastLabel = lastLabel,
            error = a?.error,
        )
    }

    private fun summary(): String {
        val st = Recorder.status.value
        val t = "%d:%02d".format(st.elapsedS / 60, st.elapsedS % 60)
        val fix = st.hAccM?.let { "±%.0f m".format(it) } ?: "no fix"
        val clock = if (st.gnssAnchors > 0) "GPS time" else "no GPS time"
        return "$t · $fix · $clock · ${st.labels} labels"
    }

    fun label(kind: String, text: String): Boolean {
        val s = store ?: return false
        val mono = SystemClock.elapsedRealtimeNanos()
        val fix = gnss?.lastFix
        val o = JSONObject().put("mono_ns", mono).put("unix_ms", System.currentTimeMillis())
            .put("kind", kind).put("text", text)
        sensors?.pointing(fix?.latitude, fix?.longitude, fix?.altitude)?.let { (az, el) ->
            o.put("azimuth_deg", az).put("pitch_deg", el)
        }
        s.labels.json(o)
        s.labels.flush()
        labelCount++
        lastLabel = kind
        publish()
        return true
    }

    private fun stop() {
        handler.removeCallbacksAndMessages(null)
        val err = audio?.error ?: Recorder.status.value.error
        runCatching { audio?.stop() }
        runCatching { gnss?.stop() }
        runCatching { sensors?.stop() }
        runCatching { store?.close() }
        wakeLock?.takeIf { it.isHeld }?.release()
        Recorder.active = null
        Recorder.mutable.value = Status(error = err)
        store = null; audio = null; gnss = null; sensors = null
        if (::thread.isInitialized) thread.quitSafely()
        stopForeground(STOP_FOREGROUND_REMOVE)
        stopSelf()
    }

    override fun onDestroy() {
        if (store != null) stop()
        super.onDestroy()
    }

    private fun recorderInfo() = JSONObject()
        .put("app", "echolocate-recorder")
        .put("app_version", BuildConfig.VERSION_NAME)
        .put("platform", "android")
        .put("os_version", Build.VERSION.RELEASE)
        .put("sdk_int", Build.VERSION.SDK_INT)
        .put("manufacturer", Build.MANUFACTURER)
        .put("model", Build.MODEL)
        .put("device", Build.DEVICE)
        .put("gnss_hardware", getSystemService(LocationManager::class.java).gnssHardwareModelName ?: JSONObject.NULL)

    private fun notification(text: String): Notification {
        val nm = getSystemService(NotificationManager::class.java)
        if (nm.getNotificationChannel(CHANNEL) == null) {
            nm.createNotificationChannel(NotificationChannel(CHANNEL, "Recording", NotificationManager.IMPORTANCE_LOW))
        }
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE
        )
        return Notification.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_btn_speak_now)
            .setContentTitle("Echolocate recording")
            .setContentText(text)
            .setContentIntent(open)
            .setOngoing(true)
            .build()
    }

    companion object {
        const val ACTION_START = "io.github.codyps.echolocate.recorder.START"
        const val ACTION_STOP = "io.github.codyps.echolocate.recorder.STOP"
        const val EXTRA_RATE = "rate"
        const val EXTRA_CHANNELS = "channels"
        const val EXTRA_MIC_HEIGHT = "mic_height_m"
        const val EXTRA_NOTES = "notes"
        private const val CHANNEL = "recording"
        private const val NOTIFICATION_ID = 1
    }
}
