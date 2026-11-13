package io.github.codyps.echolocate.recorder

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.io.RandomAccessFile
import kotlin.math.PI
import kotlin.math.sin

class SessionStoreTest {
    private val rate = 8000

    private fun store(dir: File, maxBytes: Long) = SessionStore(
        dir, dir.name, 1_762_200_000_000L,
        JSONObject().put("platform", "jvm-test").put("os_version", "0").put("model", "test"),
        JSONObject().put("sample_rate_hz", rate).put("channels", 1).put("encoding", "f32le").put("container", "wav"),
        rate, 1, 1.5, "unit test", maxBytes,
    )

    /**
     * Writes build/test-session: 25 s of a 440 Hz tone split across 3 segments, with clock,
     * location and label records shaped like the device's. `echo-locate session-info` reads it.
     */
    @Test
    fun segmentsAreContiguousHashedAndReadable() {
        val dir = File("build/test-session").apply { deleteRecursively() }
        val s = store(dir, maxBytes = WavWriter.HEADER_BYTES + 10L * rate * 4)
        val boot0 = 5_000_000_000_000L
        val unix0 = 1_762_200_000L * 1_000_000_000L
        val buf = FloatArray(rate / 10)
        var frame = 0L
        repeat(250) { block ->
            for (i in buf.indices) buf[i] = (0.25 * sin(2 * PI * 440 * (frame + i) / rate)).toFloat()
            s.writeAudio(buf, buf.size)
            frame += buf.size
            if (block % 10 == 0) {
                val mono = boot0 + frame * 1_000_000_000L / rate
                s.clock.json(JSONObject().put("type", "audio").put("frame", frame).put("mono_ns", mono).put("read", frame))
                val gpsNs = unix0 + (mono - boot0) - (315_964_800L - 18) * 1_000_000_000L
                val tn = 1_000_000_000L + frame
                s.clock.json(
                    JSONObject().put("type", "gnss").put("mono_ns", mono).put("time_nanos", tn)
                        .put("full_bias_nanos", tn - gpsNs).put("bias_nanos", 0.25).put("leap_s", 18)
                )
                s.location.json(
                    JSONObject().put("mono_ns", mono).put("provider", "gps").put("lat", 40.87).put("lon", -73.52)
                        .put("h_ellipsoid_m", -10.5).put("h_acc_m", 3.0).put("v_acc_m", 5.0)
                )
            }
        }
        s.labels.json(JSONObject().put("mono_ns", boot0 + 12_000_000_000L).put("kind", "aircraft").put("azimuth_deg", 90.0))
        s.close()

        val m = JSONObject(File(dir, "session.json").readText())
        assertTrue(m.getBoolean("complete"))
        val segs = m.getJSONArray("segments")
        assertEquals(3, segs.length())
        var next = 0L
        for (i in 0 until segs.length()) {
            val seg = segs.getJSONObject(i)
            assertEquals(next, seg.getLong("first_frame"))
            val f = File(dir, seg.getString("file"))
            assertEquals(WavWriter.HEADER_BYTES + seg.getLong("frames") * 4, f.length())
            assertEquals(SessionStore.sha256(f), seg.getString("sha256"))
            next += seg.getLong("frames")
        }
        assertEquals(250L * rate / 10, next)
        assertTrue(File(dir, "gnss_raw.txt").readText().contains("# Raw,utcTimeMillis,TimeNanos"))
    }

    @Test
    fun crashedSessionIsRepaired() {
        val dir = File("build/test-crash").apply { deleteRecursively() }
        val s = store(dir, maxBytes = 1L shl 20)
        val buf = FloatArray(rate) { 0.1f }
        repeat(3) { s.writeAudio(buf, buf.size) }
        s.flushLogs()
        // Simulate death: header still claims an earlier size, and a torn half-frame at the end.
        val wav = File(dir, "audio/seg-00000.wav")
        RandomAccessFile(wav, "rw").use { it.seek(it.length()); it.write(byteArrayOf(1, 2)) }
        assertFalse(JSONObject(File(dir, "session.json").readText()).getBoolean("complete"))

        assertTrue(SessionStore.repair(dir))
        val m = JSONObject(File(dir, "session.json").readText())
        assertFalse(m.getBoolean("complete"))
        assertTrue(m.has("repaired_unix_ms"))
        val seg = m.getJSONArray("segments").getJSONObject(0)
        assertEquals(3L * rate, seg.getLong("frames"))
        assertEquals(WavWriter.HEADER_BYTES + 3L * rate * 4, wav.length())
        val header = wav.readBytes().copyOfRange(0, WavWriter.HEADER_BYTES)
        assertTrue(header.contentEquals(WavWriter.header(rate, 1, 3L * rate)))
    }
}
