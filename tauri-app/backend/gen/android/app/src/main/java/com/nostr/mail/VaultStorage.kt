package com.nostr.mail

import android.content.Context
import androidx.security.crypto.EncryptedFile
import androidx.security.crypto.MasterKey
import java.io.File

/**
 * Encrypted-at-rest storage for the keypair vault. The master key lives in
 * the Android Keystore (TEE/StrongBox where available); the file on disk is
 * AES-256-GCM ciphertext via Jetpack Security's EncryptedFile.
 *
 * Called from Rust via JNI. MainActivity.onCreate must call [init] first.
 */
object VaultStorage {
    private lateinit var appContext: Context

    private const val DIR = "nostr-mail"
    private const val FILE = "vault.bin"

    @JvmStatic
    fun init(ctx: Context) {
        appContext = ctx.applicationContext
    }

    private fun vaultFile(): File = File(appContext.filesDir, "$DIR/$FILE")

    private fun encryptedFile(): EncryptedFile {
        File(appContext.filesDir, DIR).mkdirs()
        val masterKey = MasterKey.Builder(appContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        return EncryptedFile.Builder(
            appContext,
            vaultFile(),
            masterKey,
            EncryptedFile.FileEncryptionScheme.AES256_GCM_HKDF_4KB
        ).build()
    }

    @JvmStatic
    fun read(): String? {
        if (!vaultFile().exists()) return null
        return encryptedFile().openFileInput().bufferedReader().use { it.readText() }
    }

    @JvmStatic
    fun write(json: String) {
        // EncryptedFile refuses to overwrite an existing file.
        vaultFile().delete()
        encryptedFile().openFileOutput().use { it.write(json.toByteArray(Charsets.UTF_8)) }
    }

    @JvmStatic
    fun clear() {
        vaultFile().delete()
    }
}
