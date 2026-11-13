package io.github.codyps.echolocate.recorder

import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedWriter
import java.io.Closeable
import java.io.File
import java.io.FileOutputStream
import java.io.OutputStreamWriter
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** Thread-safe append-only line log. Lines become durable at [flush]. */
class LineLog(file: File, header: String? = null) : Closeable {
    private val out = BufferedWriter(OutputStreamWriter(FileOutputStream(file, true), Charsets.UTF_8), 1 shl 16)
    var lines = 0L
        private set

    init {
        if (header != null && file.length() == 0L) out.write(header)
    }

    @Synchronized
    fun line(s: String) {
        out.write(s)
        out.write('\n'.code)
        lines++
    }

    fun json(o: JSONObject) = line(o.toString())

    @Synchronized
    fun flush() = out.flush()

    @Synchronized
    override fun close() = out.close()
}

/**
 * One recording session on disk, laid out as docs/SESSIONS.md describes. Audio is split into
 * sample-contiguous WAV segments below [maxSegmentBytes]; the manifest is rewritten whenever a
 * segment opens or closes, and at [close].
 *
 * Platform-independent so it runs in JVM unit tests; Android specifics are passed in as JSON.
 */
class SessionStore(
    val dir: File,
    val sessionId: String,
    private val createdUnixMs: Long,
    private val recorder: JSONObject,
    private val audio: JSONObject,
    val rate: Int,
    val channels: Int,
    private val micHeightM: Double?,
    private val notes: String,
    maxSegmentBytes: Long = 48L shl 20,
) : Closeable {
    val framesPerSegment = (maxSegmentBytes - WavWriter.HEADER_BYTES) / (4L * channels)
    private val segments = mutableListOf<JSONObject>()
    private var writer: WavWriter? = null
    private var segmentFirst = 0L
    private var lastPatchFrames = 0L
    private val hasher = Executors.newSingleThreadExecutor()

    /** Frames written across all segments. */
    @Volatile
    var totalFrames = 0L
        private set
    val segmentCount get() = synchronized(this) { segments.size }

    val clock: LineLog
    val location: LineLog
    val sensors: LineLog
    val labels: LineLog
    val gnssRaw: LineLog

    init {
        File(dir, "audio").mkdirs()
        clock = LineLog(File(dir, "clock.jsonl"))
        location = LineLog(File(dir, "location.jsonl"))
        sensors = LineLog(File(dir, "sensors.jsonl"))
        labels = LineLog(File(dir, "labels.jsonl"))
        gnssRaw = LineLog(File(dir, "gnss_raw.txt"), GnssRaw.header(recorder))
        writeManifest(false)
    }

    /** Append interleaved samples (whole frames). Call from one thread only. */
    fun writeAudio(buf: FloatArray, samples: Int) {
        var off = 0
        while (off < samples) {
            val w = writer ?: openSegment()
            val room = (framesPerSegment - w.frames) * channels
            val n = minOf(samples - off, room.toInt())
            w.write(buf, off, n)
            off += n
            totalFrames += n / channels
            if (w.frames >= framesPerSegment) closeSegment()
        }
        // Keep the header (and so a crash-recovered file) within ~10 s of the data.
        val w = writer
        if (w != null && totalFrames - lastPatchFrames >= 10L * rate) {
            w.patchHeader()
            lastPatchFrames = totalFrames
        }
    }

    private fun openSegment(): WavWriter {
        val name = "audio/seg-%05d.wav".format(segments.size)
        val w = WavWriter(File(dir, name), rate, channels)
        synchronized(this) {
            segmentFirst = totalFrames
            segments += JSONObject().put("file", name).put("first_frame", segmentFirst).put("frames", 0)
        }
        writer = w
        writeManifest(false)
        return w
    }

    private fun closeSegment() {
        val w = writer ?: return
        writer = null
        w.close()
        val seg = synchronized(this) { segments.last().put("frames", w.frames) }
        val file = File(dir, seg.getString("file"))
        // Hash off the audio thread; the AudioRecord buffer absorbs the close itself.
        hasher.execute {
            val sha = sha256(file)
            synchronized(this) { seg.put("sha256", sha) }
            writeManifest(false)
        }
    }

    fun flushLogs() {
        for (l in listOf(clock, location, sensors, labels, gnssRaw)) l.flush()
    }

    override fun close() {
        closeSegment()
        hasher.shutdown()
        hasher.awaitTermination(2, TimeUnit.MINUTES)
        for (l in listOf(clock, location, sensors, labels, gnssRaw)) l.close()
        writeManifest(true)
    }

    @Synchronized
    private fun writeManifest(complete: Boolean) {
        val m = JSONObject()
            .put("format", "echolocate.session")
            .put("format_version", 1)
            .put("session_id", sessionId)
            .put("created_unix_ms", createdUnixMs)
            .put("complete", complete)
            .put("monotonic_clock", "android_elapsed_realtime_ns")
            .put("recorder", recorder)
            .put("audio", audio)
            .put("segments", JSONArray(segments.map { JSONObject(it.toString()) }))
            .put("stationary", true)
            .put("mic_height_above_ground_m", micHeightM ?: JSONObject.NULL)
            .put("site_override", JSONObject.NULL)
            .put("notes", notes)
        writeAtomically(File(dir, "session.json"), m.toString(2))
    }

    companion object {
        fun sha256(file: File): String {
            val md = MessageDigest.getInstance("SHA-256")
            file.inputStream().use { s ->
                val buf = ByteArray(1 shl 16)
                while (true) {
                    val n = s.read(buf)
                    if (n < 0) break
                    md.update(buf, 0, n)
                }
            }
            return md.digest().joinToString("") { "%02x".format(it) }
        }

        fun writeAtomically(target: File, text: String) {
            val tmp = File(target.parentFile, target.name + ".tmp")
            tmp.writeText(text)
            if (!tmp.renameTo(target)) {
                target.delete()
                check(tmp.renameTo(target)) { "could not replace $target" }
            }
        }

        fun readManifest(dir: File): JSONObject? =
            runCatching { JSONObject(File(dir, "session.json").readText()) }.getOrNull()

        /**
         * Recover a session whose recorder died: fix every WAV header, rebuild the segment list
         * from the files on disk, hash them, and mark the manifest `repaired`. `complete` stays
         * false so analysis still warns that the tail may be missing.
         */
        fun repair(dir: File): Boolean {
            val m = readManifest(dir) ?: return false
            if (m.optBoolean("complete")) return false
            val audio = m.getJSONObject("audio")
            val rate = audio.getInt("sample_rate_hz")
            val channels = audio.getInt("channels")
            val files = File(dir, "audio").listFiles { f -> f.name.matches(Regex("seg-\\d{5}\\.wav")) }
                ?.sortedBy { it.name } ?: emptyList()
            val segs = JSONArray()
            var first = 0L
            for (f in files) {
                val frames = WavWriter.repair(f, rate, channels)
                if (frames == 0L) {
                    f.delete()
                    continue
                }
                segs.put(
                    JSONObject().put("file", "audio/" + f.name).put("first_frame", first)
                        .put("frames", frames).put("sha256", sha256(f))
                )
                first += frames
            }
            m.put("segments", segs).put("repaired_unix_ms", System.currentTimeMillis())
            writeAtomically(File(dir, "session.json"), m.toString(2))
            return true
        }
    }
}
