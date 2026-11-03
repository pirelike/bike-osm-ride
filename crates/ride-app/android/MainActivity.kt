package dev.dioxus.main

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import java.io.File

/**
 * `dx` puts this in the generated `MainActivity.kt` and the generated `Logger.kt` (from wry) uses
 * it. The application id is the one in `Dioxus.toml` (`bundle.identifier`).
 */
typealias BuildConfig = hu.borbelyhuba.bikeosmride.BuildConfig

/**
 * The activity `dx` generates is `class MainActivity : WryActivity()`; this one adds the two things
 * a desktop-first app needs on a phone and the generated file has no place for:
 *
 *  * a real GPX file picker (Storage Access Framework). The picker hands back a `content://` URI,
 *    which the route loader cannot read as a path, so the stream is copied into the app's own
 *    cache folder first and that path goes to Rust through `onPicked`;
 *  * the runtime Bluetooth permissions (Android 12+ needs `BLUETOOTH_SCAN`/`_CONNECT`, older
 *    versions `ACCESS_FINE_LOCATION` for a scan). Declaring them in the manifest is not enough.
 *
 * Both are driven from Rust through JNI (`ride-app/src/android_picker.rs`), and both are called on
 * the main thread because that is where Android wants them.
 */
class MainActivity : WryActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Asked once, early: the sensor hub starts on its own thread right behind this, and the
        // dialog is answered while the webview is still loading.
        requestBlePermissions()
    }

    /** Opens the system file picker for a GPX; the answer arrives in `onPicked`. */
    fun pickGpx() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf(
                    "application/gpx+xml",
                    "application/xml",
                    "text/xml",
                    "text/plain",
                    "application/octet-stream",
                ),
            )
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, REQ_PICK_GPX)
        } catch (e: Exception) {
            // No picker on this device (or it refused): tell Rust, which shows the reason.
            onPicked("")
        }
    }

    /** The Bluetooth permissions the scan needs, per API level. */
    fun hasBlePermissions(): Boolean =
        blePermissions().all { checkSelfPermission(it) == PackageManager.PERMISSION_GRANTED }

    fun requestBlePermissions() {
        if (hasBlePermissions()) return
        try {
            requestPermissions(blePermissions(), REQ_BLE)
        } catch (e: Exception) {
            // Nothing to ask (or the system refuses): the hub reports what it can do.
        }
    }

    @Deprecated("The picker is started with startActivityForResult; there is no registerForActivityResult here.")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQ_PICK_GPX) return
        val uri = if (resultCode == RESULT_OK) data?.data else null
        if (uri == null) {
            onPicked("")
            return
        }
        onPicked(copyIntoCache(uri))
    }

    /** A picked document is a `content://` URI: copy it into the cache and hand back a real path. */
    private fun copyIntoCache(uri: Uri): String = try {
        val dir = File(cacheDir, "gpx").apply { mkdirs() }
        val name = displayName(uri) ?: "utvonal.gpx"
        val safe = name.replace(Regex("[^A-Za-z0-9._ -]"), "_").takeLast(80)
        val file = File(dir, if (safe.lowercase().endsWith(".gpx")) safe else "$safe.gpx")
        contentResolver.openInputStream(uri)?.use { input ->
            file.outputStream().use { output -> input.copyTo(output) }
        }
        file.absolutePath
    } catch (e: Exception) {
        ""
    }

    private fun displayName(uri: Uri): String? =
        try {
            contentResolver.query(uri, null, null, null, null)?.use { cursor ->
                val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (index >= 0 && cursor.moveToFirst()) cursor.getString(index) else null
            }
        } catch (e: Exception) {
            null
        }

    private fun blePermissions(): Array<String> =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
        } else {
            arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
        }

    /** Implemented in Rust (`Java_dev_dioxus_main_MainActivity_onPicked`); "" means cancelled. */
    private external fun onPicked(path: String)

    companion object {
        private const val REQ_PICK_GPX = 4671
        private const val REQ_BLE = 4672
    }
}
