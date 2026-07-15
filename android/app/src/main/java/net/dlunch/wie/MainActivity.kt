package net.dlunch.wie

import androidx.activity.ComponentActivity
import android.net.Uri
import android.os.Bundle
import android.widget.Toast
import androidx.activity.result.contract.ActivityResultContracts

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
        setContentView(R.layout.activity_main)

        findViewById<android.view.View>(R.id.pick_rom_button).setOnClickListener {
            pickFile.launch("*/*")
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
