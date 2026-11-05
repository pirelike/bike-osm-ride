package dev.dioxus.main

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import android.view.WindowManager
import java.io.File
import kotlin.concurrent.thread

/**
 * `dx` puts this in the generated `MainActivity.kt` and the generated `Logger.kt` (from wry) uses
 * it. The application id is the one in `Dioxus.toml` (`bundle.identifier`).
 */
typealias BuildConfig = hu.borbelyhuba.bikeosmride.BuildConfig

/**
 * The activity `dx` generates is `class MainActivity : WryActivity()`; this one adds the things
 * a desktop-first app needs on a phone and the generated file has no place for:
 *
 *  * a real GPX file picker (Storage Access Framework). The picker hands back a `content://` URI,
 *    which the route loader cannot read as a path, so the stream is copied into the app's own
 *    cache folder first and that path goes to Rust through `onPicked`;
 *  * the same for a backup zip, both ways: `restoreBackup` picks one (copied into the cache,
 *    answered through `onBackupPicked`), `saveFile` hands a file Rust wrote (the backup zip, a
 *    ride's TCX) to the system's "save as" dialog (answered through `onBackupSaved`). The copies
 *    run on a worker thread, the files can be tens of megabytes;
 *  * the runtime Bluetooth permissions (Android 12+ needs `BLUETOOTH_SCAN`/`_CONNECT`, older
 *    versions `ACCESS_FINE_LOCATION` for a scan). Declaring them in the manifest is not enough;
 *  * the keep-screen-on flag while a ride runs.
 *
 * All of it is driven from Rust through JNI (`ride-app/src/android_picker.rs`). The pickers and the
 * permission dialog are started on the main thread because that is where Android wants them.
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

    /** Opens the system file picker for a backup zip; the answer arrives in `onBackupPicked`. */
    fun restoreBackup() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(
                Intent.EXTRA_MIME_TYPES,
                arrayOf("application/zip", "application/x-zip-compressed", "application/octet-stream"),
            )
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, REQ_PICK_BACKUP)
        } catch (e: Exception) {
            onBackupPicked("")
        }
    }

    /**
     * Offers the file Rust wrote at `path` (a backup zip, a TCX) to the system's "save as" dialog.
     * Once the rider chose a place the file is copied there and `path` is deleted; `onBackupSaved`
     * says how it went. `mime` is the dialog's type filter; for a type Android has no extension for
     * (`application/octet-stream`) the dialog keeps `name` exactly as given.
     */
    fun saveFile(path: String, name: String, mime: String) {
        pendingBackup = path
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = mime
            putExtra(Intent.EXTRA_TITLE, name)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, REQ_SAVE_BACKUP)
        } catch (e: Exception) {
            finishBackupSave(null)
        }
    }

    /**
     * Keeps the screen on while a ride runs. Rust calls this from its own thread, and a window flag
     * may only change on the UI thread; the flag lives on the window, so nothing has to be undone
     * when the app goes away.
     */
    fun setKeepScreenOn(on: Boolean) {
        runOnUiThread {
            try {
                if (on) {
                    window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                } else {
                    window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                }
            } catch (e: Exception) {
                // No window (yet, or any more): the system's own screen timeout applies.
            }
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
        val uri = if (resultCode == RESULT_OK) data?.data else null
        when (requestCode) {
            REQ_PICK_GPX -> onPicked(if (uri == null) "" else copyIntoCache(uri))
            REQ_PICK_BACKUP -> {
                if (uri == null) {
                    onBackupPicked("")
                } else {
                    thread { onBackupPicked(copyIntoCache(uri, "backup", "mentes.zip", ".zip")) }
                }
            }
            REQ_SAVE_BACKUP -> {
                if (uri == null) {
                    finishBackupSave(null)
                } else {
                    thread { finishBackupSave(uri) }
                }
            }
        }
    }

    /** The zip Rust wrote, waiting for the rider to say where it goes. */
    private var pendingBackup: String? = null

    /**
     * Copies the pending zip to the place the rider chose (`uri`; null: nothing chosen), removes it
     * from the cache either way and tells Rust: `ok`, `cancel` or `error`.
     */
    private fun finishBackupSave(uri: Uri?) {
        val source = pendingBackup?.let { File(it) }
        pendingBackup = null
        val result = try {
            if (uri == null) {
                "cancel"
            } else if (source == null || !source.isFile) {
                "error"
            } else {
                // "wt": an existing file the rider picked is replaced, not appended to
                val output = contentResolver.openOutputStream(uri, "wt")
                if (output == null) {
                    "error"
                } else {
                    output.use { out -> source.inputStream().use { input -> input.copyTo(out) } }
                    "ok"
                }
            }
        } catch (e: Exception) {
            "error"
        }
        try {
            source?.delete()
        } catch (e: Exception) {
            // the cache is cleaned by the system sooner or later
        }
        onBackupSaved(result)
    }

    /** A picked document is a `content://` URI: copy it into the cache and hand back a real path. */
    private fun copyIntoCache(
        uri: Uri,
        folder: String = "gpx",
        fallbackName: String = "utvonal.gpx",
        extension: String = ".gpx",
    ): String = try {
        val dir = File(cacheDir, folder).apply { mkdirs() }
        val name = displayName(uri) ?: fallbackName
        val safe = name.replace(Regex("[^A-Za-z0-9._ -]"), "_").takeLast(80)
        val file = File(dir, if (safe.lowercase().endsWith(extension)) safe else "$safe$extension")
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

    /** Implemented in Rust (`..._onBackupPicked`); "" means cancelled or the copy failed. */
    private external fun onBackupPicked(path: String)

    /** Implemented in Rust (`..._onBackupSaved`); `ok`, `cancel` or `error`. */
    private external fun onBackupSaved(result: String)

    companion object {
        private const val REQ_PICK_GPX = 4671
        private const val REQ_BLE = 4672
        private const val REQ_PICK_BACKUP = 4673
        private const val REQ_SAVE_BACKUP = 4674
    }
}
