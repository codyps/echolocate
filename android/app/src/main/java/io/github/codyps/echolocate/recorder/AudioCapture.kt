package io.github.codyps.echolocate.recorder

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTimestamp
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AudioEffect
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import android.os.Process
import android.os.SystemClock
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.sqrt

/**
 * Raw microphone capture. Prefers the UNPROCESSED source (no AGC, noise suppression or
 * high-pass), records 32-bit float, and logs converter timestamps in the boot-time base so
 * analysis can map each sample to GPS time.
 */
class AudioCapture(private val am: AudioManager, val rate: Int, val channels: Int) {
    val unprocessedSupported =
        am.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED) == "true"
    private val source =
        if (unprocessedSupported) MediaRecorder.AudioSource.UNPROCESSED else MediaRecorder.AudioSource.VOICE_RECOGNITION
    private val mask = if (channels == 2) AudioFormat.CHANNEL_IN_STEREO else AudioFormat.CHANNEL_IN_MONO
    private lateinit var record: AudioRecord
    private val effects = mutableListOf<AudioEffect>()
    private val disabled = mutableListOf<String>()
    private var thread: Thread? = null
    @Volatile private var running = false
    @Volatile var error: String? = null
        private set

    // Level meter over the last ~0.5 s, read by the UI.
    @Volatile var peakDbfs = -120.0
        private set
    @Volatile var rmsDbfs = -120.0
        private set
    @Volatile var clippedSamples = 0L
        private set

    /** Create and start the recorder; returns the `audio` object for session.json. */
    @SuppressLint("MissingPermission") // Checked by MainActivity before the service starts.
    fun prepare(): JSONObject {
        val min = AudioRecord.getMinBufferSize(rate, mask, AudioFormat.ENCODING_PCM_FLOAT)
        check(min > 0) { "Device rejects $rate Hz float capture ($min)" }
        // Two seconds of buffer rides out segment rollover and GC pauses.
        val bytes = max(min, rate * channels * 4 * 2)
        record = AudioRecord.Builder()
            .setAudioSource(source)
            .setAudioFormat(
                AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                    .setSampleRate(rate).setChannelMask(mask).build()
            )
            .setBufferSizeInBytes(bytes)
            .build()
        check(record.state == AudioRecord.STATE_INITIALIZED) { "AudioRecord failed to initialize" }
        disableEffects()
        record.startRecording()
        val mics = JSONArray()
        runCatching {
            for (m in record.activeMicrophones) {
                mics.put(
                    JSONObject().put("description", m.description).put("id", m.id).put("type", m.type)
                        .put("location", m.location).put("directionality", m.directionality)
                        .put("position_m", JSONArray(listOf(m.position.x, m.position.y, m.position.z)))
                )
            }
        }
        val dev = record.routedDevice
        return JSONObject()
            .put("source", if (unprocessedSupported) "UNPROCESSED" else "VOICE_RECOGNITION")
            .put("unprocessed_supported", unprocessedSupported)
            .put("sample_rate_hz", record.sampleRate)
            .put("channels", channels)
            .put("encoding", "f32le")
            .put("container", "wav")
            .put("buffer_frames", bytes / (4 * channels))
            .put("input_device", dev?.let { "${it.productName} (type ${it.type})" } ?: JSONObject.NULL)
            .put("microphones", mics)
            .put("effects_disabled", JSONArray(disabled))
            .put("gain", "fixed (no software gain applied)")
    }

    private fun disableEffects() {
        val id = record.audioSessionId
        fun <T : AudioEffect> off(name: String, available: Boolean, make: () -> T?) {
            if (!available) return
            val e = runCatching { make() }.getOrNull() ?: return
            if (e.enabled) {
                e.enabled = false
                disabled += name
            }
            effects += e // Held until stop so the effect stays detached.
        }
        off("AutomaticGainControl", AutomaticGainControl.isAvailable()) { AutomaticGainControl.create(id) }
        off("NoiseSuppressor", NoiseSuppressor.isAvailable()) { NoiseSuppressor.create(id) }
        off("AcousticEchoCanceler", AcousticEchoCanceler.isAvailable()) { AcousticEchoCanceler.create(id) }
    }

    fun start(store: SessionStore) {
        running = true
        thread = Thread({ loop(store) }, "echolocate-audio").also { it.start() }
    }

    private fun loop(store: SessionStore) {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        val buf = FloatArray(rate / 10 * channels) // 100 ms reads
        val ts = AudioTimestamp()
        var read = 0L
        var lastStamp = 0L
        var meterPeak = 0f
        var meterSq = 0.0
        var meterN = 0
        try {
            while (running) {
                val n = record.read(buf, 0, buf.size, AudioRecord.READ_BLOCKING)
                if (n < 0) {
                    error = "AudioRecord.read error $n"
                    break
                }
                val whole = n - n % channels
                store.writeAudio(buf, whole)
                read += whole / channels
                for (i in 0 until whole) {
                    val a = abs(buf[i])
                    if (a > meterPeak) meterPeak = a
                    meterSq += a * a
                    if (a >= 0.999f) clippedSamples++
                }
                meterN += whole
                if (meterN >= rate * channels / 2) {
                    peakDbfs = 20 * log10(max(meterPeak.toDouble(), 1e-6))
                    rmsDbfs = 10 * log10(max(meterSq / meterN, 1e-12))
                    meterPeak = 0f; meterSq = 0.0; meterN = 0
                }
                val now = SystemClock.elapsedRealtime()
                if (now - lastStamp >= 1000 &&
                    record.getTimestamp(ts, AudioTimestamp.TIMEBASE_BOOTTIME) == AudioRecord.SUCCESS
                ) {
                    store.clock.json(
                        JSONObject().put("type", "audio").put("frame", ts.framePosition)
                            .put("mono_ns", ts.nanoTime).put("read", read)
                    )
                    lastStamp = now
                }
            }
        } catch (t: Throwable) {
            error = "Audio thread: $t"
        }
    }

    fun stop() {
        running = false
        thread?.join(3000)
        runCatching { record.stop() }
        record.release()
        effects.forEach { it.release() }
    }
}
