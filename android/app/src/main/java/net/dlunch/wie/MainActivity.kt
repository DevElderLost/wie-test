package net.dlunch.wie

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.ActivityCompat

/**
 * MVP shell: pick a .zip (KTF/LGT/SKT archive) or .jar from storage, load
 * it, and start ticking. No ROM library / recent-files UI yet - that's a
 * good next step once the core loop is confirmed working on-device.
 */
class MainActivity : ComponentActivity() {

    private var loaded = false

    private val pickFile = registerForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri != null) loadApp(uri)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Must run before anything touches `WieNative` (which happens as
        // soon as setContentView() below inflates VirtualKeypadView) -
        // otherwise wie_jni's log file open fails silently on first launch
        // because the permission isn't granted yet. If that happens, grant
        // the permission and just relaunch the app once.
        ensureStoragePermission()

        setContentView(R.layout.activity_main)

        findViewById<android.view.View>(R.id.pick_rom_button).setOnClickListener {
            pickFile.launch("*/*")
        }
    }

    private fun ensureStoragePermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            if (!Environment.isExternalStorageManager()) {
                Toast.makeText(this, "Izinkan \"All files access\" biar log bisa ditulis ke /storage/emulated/0/wie/", Toast.LENGTH_LONG).show()
                val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:$packageName"))
                startActivity(intent)
            }
        } else {
            if (ActivityCompat.checkSelfPermission(this, Manifest.permission.WRITE_EXTERNAL_STORAGE) != PackageManager.PERMISSION_GRANTED) {
                ActivityCompat.requestPermissions(this, arrayOf(Manifest.permission.WRITE_EXTERNAL_STORAGE), 1001)
            }
        }
    }

    private fun loadApp(uri: Uri) {
        val filename = queryDisplayName(uri) ?: "rom.zip"
        val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() }
        if (bytes == null) {
            Toast.makeText(this, "Gagal membaca file", Toast.LENGTH_SHORT).show()
            return
        }

        val ok = WieNative.nativeLoadApp(filesDir.absolutePath, filename, bytes)
        if (!ok) {
            Toast.makeText(this, "Gagal load ROM (cek logcat tag wie_jni)", Toast.LENGTH_LONG).show()
            return
        }

        loaded = true
        WieNative.nativeStart()
        findViewById<android.view.View>(R.id.pick_rom_button).visibility = android.view.View.GONE
    }

    private fun queryDisplayName(uri: Uri): String? {
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val nameIndex = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (nameIndex >= 0 && cursor.moveToFirst()) {
                return cursor.getString(nameIndex)
            }
        }
        return null
    }

    override fun onPause() {
        super.onPause()
        if (loaded) WieNative.nativeStop()
    }

    override fun onResume() {
        super.onResume()
        if (loaded) WieNative.nativeStart()
    }

    override fun onDestroy() {
        super.onDestroy()
        if (loaded) WieNative.nativeDestroy()
    }
}
