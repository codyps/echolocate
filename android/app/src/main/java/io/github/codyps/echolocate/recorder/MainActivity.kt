package io.github.codyps.echolocate.recorder

import android.Manifest
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.Environment
import android.provider.MediaStore
import android.view.HapticFeedbackConstants
import android.view.WindowManager
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

class MainActivity : ComponentActivity() {
    private val permissions = arrayOf(
        Manifest.permission.RECORD_AUDIO,
        Manifest.permission.ACCESS_FINE_LOCATION,
        Manifest.permission.POST_NOTIFICATIONS,
    )
    private var pendingStart: (() -> Unit)? = null
    private val request = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
        if (r[Manifest.permission.RECORD_AUDIO] == true && r[Manifest.permission.ACCESS_FINE_LOCATION] == true) {
            pendingStart?.invoke()
        } else {
            Toast.makeText(this, "Microphone and precise location are required", Toast.LENGTH_LONG).show()
        }
        pendingStart = null
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // A stationary session is usually unattended on a tripod: keep the status visible.
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                Surface(Modifier.fillMaxSize()) { Screen(::startRecording, ::stopRecording) }
            }
        }
    }

    private fun startRecording(channels: Int, micHeight: Double?, notes: String) {
        val go: () -> Unit = {
            val i = Intent(this, RecorderService::class.java).setAction(RecorderService.ACTION_START)
                .putExtra(RecorderService.EXTRA_RATE, 48_000)
                .putExtra(RecorderService.EXTRA_CHANNELS, channels)
                .putExtra(RecorderService.EXTRA_MIC_HEIGHT, micHeight ?: Double.NaN)
                .putExtra(RecorderService.EXTRA_NOTES, notes)
            startForegroundService(i)
        }
        val missing = permissions.filter { checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }
        if (missing.isEmpty()) go() else {
            pendingStart = go
            request.launch(missing.toTypedArray())
        }
    }

    private fun stopRecording() {
        startService(Intent(this, RecorderService::class.java).setAction(RecorderService.ACTION_STOP))
    }
}

private data class SessionRow(val dir: File, val complete: Boolean, val repaired: Boolean, val bytes: Long)

private fun listSessions(ctx: Context): List<SessionRow> {
    val activeId = Recorder.status.value.sessionId
    return Recorder.sessionsDir(ctx).listFiles { f -> f.isDirectory }.orEmpty()
        .sortedByDescending { it.name }
        .map { d ->
            // Recover sessions whose recorder died, but never the one being written.
            if (d.name != activeId) SessionStore.repair(d)
            val m = SessionStore.readManifest(d)
            SessionRow(d, m?.optBoolean("complete") == true, m?.has("repaired_unix_ms") == true,
                d.walkTopDown().filter { it.isFile }.sumOf { it.length() })
        }
}

/** Zip a session into Downloads/Echolocate/<id>.zip (WAV is stored, logs are deflated). */
private fun export(ctx: Context, dir: File): String {
    val values = ContentValues().apply {
        put(MediaStore.Downloads.DISPLAY_NAME, "${dir.name}.zip")
        put(MediaStore.Downloads.MIME_TYPE, "application/zip")
        put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Echolocate")
    }
    val uri = ctx.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        ?: error("Could not create download")
    ctx.contentResolver.openOutputStream(uri)!!.use { os ->
        ZipOutputStream(os.buffered(1 shl 16)).use { zip ->
            for (f in dir.walkTopDown().filter { it.isFile && !it.name.endsWith(".tmp") }) {
                val entry = ZipEntry(dir.name + "/" + f.relativeTo(dir).invariantSeparatorsPath)
                zip.setLevel(if (f.extension == "wav") 0 else 6)
                zip.putNextEntry(entry)
                f.inputStream().use { it.copyTo(zip, 1 shl 16) }
                zip.closeEntry()
            }
        }
    }
    return "Downloads/Echolocate/${dir.name}.zip"
}

@Composable
private fun Screen(start: (Int, Double?, String) -> Unit, stop: () -> Unit) {
    val st by Recorder.status.collectAsState()
    val ctx = LocalView.current.context
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    var stereo by remember { mutableStateOf(false) }
    var height by remember { mutableStateOf("1.5") }
    var notes by remember { mutableStateOf("") }
    var sessions by remember { mutableStateOf(emptyList<SessionRow>()) }
    var busy by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(st.recording) {
        sessions = withContext(Dispatchers.IO) { listSessions(ctx) }
    }

    Column(
        Modifier.safeDrawingPadding().padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Echolocate Recorder", style = MaterialTheme.typography.headlineSmall)
        st.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }

        if (!st.recording) {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("Stationary session. Put the phone on a tripod, mic unobstructed, in a windscreen if possible.")
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        FilterChip(selected = !stereo, onClick = { stereo = false }, label = { Text("Mono") })
                        FilterChip(selected = stereo, onClick = { stereo = true }, label = { Text("Stereo") })
                    }
                    OutlinedTextField(
                        height, { height = it }, label = { Text("Mic height above ground (m)") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), singleLine = true,
                    )
                    OutlinedTextField(notes, { notes = it }, label = { Text("Notes (site, weather, setup)") })
                    Button(
                        { start(if (stereo) 2 else 1, height.toDoubleOrNull(), notes) },
                        Modifier.fillMaxWidth().height(56.dp),
                    ) { Text("Start recording") }
                }
            }
        } else {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(12.dp)) {
                    val mono = FontFamily.Monospace
                    Text(st.sessionId ?: "", fontFamily = mono, fontSize = 12.sp)
                    Text("Elapsed  %d:%02d   segments %d".format(st.elapsedS / 60, st.elapsedS % 60, st.segments), fontFamily = mono)
                    Text("Audio    peak %.1f dBFS  rms %.1f dBFS  clipped %d".format(st.peakDbfs, st.rmsDbfs, st.clipped), fontFamily = mono)
                    Text("Source   " + if (st.unprocessed == true) "UNPROCESSED" else "VOICE_RECOGNITION (unprocessed unsupported)", fontFamily = mono)
                    Text(
                        "Fix      " + (st.hAccM?.let { "±%.1f m  h %.1f m (ellipsoid)".format(it, st.heightM ?: Double.NaN) } ?: "waiting…"),
                        fontFamily = mono,
                    )
                    Text("GPS time ${if (st.gnssAnchors > 0) "${st.gnssAnchors} anchors" else "not yet (wall clock only)"}", fontFamily = mono)
                    Text("Raw GNSS ${st.rawMeasurements} measurements", fontFamily = mono)
                    st.pressureHpa?.let { Text("Pressure %.2f hPa".format(it), fontFamily = mono) }
                }
            }
            Text("Labels: ${st.labels}" + (st.lastLabel?.let { " (last: $it)" } ?: ""))
            Button(
                {
                    if (Recorder.label("aircraft")) view.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
                },
                Modifier.fillMaxWidth().height(96.dp),
            ) { Text("✈  Aircraft now", fontSize = 22.sp) }
            Text("Point the top of the phone at the aircraft when you press, to log its bearing.", style = MaterialTheme.typography.bodySmall)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (k in listOf("bell", "wind", "speech", "other")) {
                    OutlinedButton({
                        if (Recorder.label(k)) view.performHapticFeedback(HapticFeedbackConstants.CONFIRM)
                    }) { Text(k) }
                }
            }
            Spacer(Modifier.height(8.dp))
            Button(
                stop, Modifier.fillMaxWidth().height(56.dp),
                colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
            ) { Text("Stop") }
        }

        Text("Sessions", style = MaterialTheme.typography.titleMedium)
        busy?.let { Text(it) }
        for (s in sessions) {
            Card(Modifier.fillMaxWidth()) {
                Row(Modifier.padding(12.dp), horizontalArrangement = Arrangement.SpaceBetween) {
                    Column(Modifier.weight(1f)) {
                        Text(s.dir.name, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                        Text(
                            "%.1f MB · %s".format(
                                s.bytes / 1e6,
                                when {
                                    s.complete -> "complete"
                                    s.repaired -> "recovered after crash"
                                    else -> "recording"
                                }
                            ),
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    OutlinedButton(
                        enabled = busy == null && s.dir.name != st.sessionId,
                        onClick = {
                            busy = "Exporting ${s.dir.name}…"
                            scope.launch {
                                val msg = withContext(Dispatchers.IO) {
                                    runCatching { "Saved " + export(ctx, s.dir) }.getOrElse { "Export failed: ${it.message}" }
                                }
                                busy = null
                                Toast.makeText(ctx, msg, Toast.LENGTH_LONG).show()
                            }
                        },
                    ) { Text("Export") }
                }
            }
        }
    }
}
