package io.github.codyps.echolocate.recorder

import java.io.Closeable
import java.io.File
import java.io.FileOutputStream
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.channels.FileChannel

/**
 * 32-bit IEEE-float WAV writer. Float keeps the recorder's full headroom (no gain staging,
 * no clipping in the container). The header is rewritten periodically so a crash loses at most
 * the unpatched tail, and [repair] can recover the rest from the file length.
 *
 * Layout: RIFF | fmt (18 bytes, WAVE_FORMAT_IEEE_FLOAT, cbSize 0) | fact | data.
 */
class WavWriter(file: File, private val rate: Int, private val channels: Int) : Closeable {
    private val channel: FileChannel = FileOutputStream(file).channel
    private val scratch = ByteBuffer.allocateDirect(64 * 1024).order(ByteOrder.LITTLE_ENDIAN)

    /** Frames (samples per channel) written so far. */
    var frames = 0L
        private set

    init {
        channel.write(ByteBuffer.wrap(header(rate, channels, 0)))
    }

    /** Write [count] interleaved samples starting at [offset]; count must be whole frames. */
    fun write(buf: FloatArray, offset: Int, count: Int) {
        require(count % channels == 0) { "partial frame" }
        var i = offset
        val end = offset + count
        while (i < end) {
            scratch.clear()
            val n = minOf(end - i, scratch.capacity() / 4)
            scratch.asFloatBuffer().put(buf, i, n)
            scratch.limit(n * 4)
            while (scratch.hasRemaining()) channel.write(scratch)
            i += n
        }
        frames += count / channels
    }

    /** Rewrite sizes in the header without moving the append position. */
    fun patchHeader() {
        channel.write(ByteBuffer.wrap(header(rate, channels, frames)), 0)
    }

    override fun close() {
        patchHeader()
        channel.force(true)
        channel.close()
    }

    companion object {
        const val HEADER_BYTES = 58

        fun header(rate: Int, channels: Int, frames: Long): ByteArray {
            val dataBytes = frames * 4 * channels
            require(dataBytes + HEADER_BYTES - 8 <= 0xFFFF_FFFFL) { "WAV too large" }
            val b = ByteBuffer.allocate(HEADER_BYTES).order(ByteOrder.LITTLE_ENDIAN)
            b.put("RIFF".toByteArray()).putInt((HEADER_BYTES - 8 + dataBytes).toInt())
            b.put("WAVE".toByteArray())
            b.put("fmt ".toByteArray()).putInt(18)
            b.putShort(3) // WAVE_FORMAT_IEEE_FLOAT
            b.putShort(channels.toShort())
            b.putInt(rate)
            b.putInt(rate * channels * 4)
            b.putShort((channels * 4).toShort())
            b.putShort(32)
            b.putShort(0) // cbSize
            b.put("fact".toByteArray()).putInt(4).putInt(frames.toInt())
            b.put("data".toByteArray()).putInt(dataBytes.toInt())
            return b.array()
        }

        /**
         * Fix the header of a file whose writer died: drop any partial trailing frame and set the
         * sizes from the file length. Returns the frame count. Only valid for files this class wrote.
         */
        fun repair(file: File, rate: Int, channels: Int): Long {
            RandomAccessFile(file, "rw").use { raf ->
                val frameBytes = 4L * channels
                val frames = maxOf(0L, raf.length() - HEADER_BYTES) / frameBytes
                raf.setLength(HEADER_BYTES + frames * frameBytes)
                raf.seek(0)
                raf.write(header(rate, channels, frames))
                raf.fd.sync()
                return frames
            }
        }
    }
}
