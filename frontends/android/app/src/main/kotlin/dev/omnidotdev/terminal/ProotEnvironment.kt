package dev.omnidotdev.terminal

import android.content.Context
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.URL
import javax.net.ssl.HttpsURLConnection
import org.tukaani.xz.XZInputStream

object ProotEnvironment {
    private const val ARCH_VERSION = "1"
    private const val ROOTFS_DIR = "archlinux"
    private const val VERSION_FILE = ".archlinux_version"
    private const val PROOT_BINARY = "usr/bin/proot"

    // proot-distro's CDN for Arch Linux ARM rootfs
    private const val ROOTFS_URL =
        "https://easycli.sh/proot-distro/archlinux-aarch64-pd-v4.37.0.tar.xz"

    fun isInstalled(context: Context): Boolean {
        val filesDir = context.filesDir
        val versionFile = File(filesDir, VERSION_FILE)
        val rootfs = File(filesDir, ROOTFS_DIR)
        return versionFile.exists()
            && rootfs.exists()
            && versionFile.readText().trim() == ARCH_VERSION
    }

    fun isProotAvailable(context: Context): Boolean {
        val proot = File(context.filesDir, PROOT_BINARY)
        return proot.exists() && proot.canExecute()
    }

    fun install(context: Context, onProgress: (String, Int) -> Unit) {
        val filesDir = context.filesDir
        val rootfsDir = File(filesDir, ROOTFS_DIR)
        val tarFile = File(filesDir, "archlinux.tar.xz")

        // Download rootfs
        onProgress("Downloading Arch Linux ARM...", 0)
        downloadFile(ROOTFS_URL, tarFile) { percent ->
            onProgress("Downloading Arch Linux ARM... $percent%", percent)
        }

        // Extract rootfs
        onProgress("Extracting rootfs...", -1)
        rootfsDir.mkdirs()
        extractTarXz(tarFile, rootfsDir, context)

        // Clean up downloaded archive
        tarFile.delete()

        // Configure pacman
        onProgress("Configuring pacman...", -1)
        configurePacman(rootfsDir)

        // Write version marker
        File(filesDir, VERSION_FILE).writeText(ARCH_VERSION)
        onProgress("Done", 100)
    }

    fun remove(context: Context) {
        val filesDir = context.filesDir
        File(filesDir, ROOTFS_DIR).deleteRecursively()
        File(filesDir, VERSION_FILE).delete()
    }

    fun rootfsPath(context: Context): String =
        File(context.filesDir, ROOTFS_DIR).absolutePath

    fun prootPath(context: Context): String =
        File(context.filesDir, PROOT_BINARY).absolutePath

    private fun downloadFile(
        urlStr: String,
        dest: File,
        onPercent: (Int) -> Unit,
    ) {
        val url = URL(urlStr)
        val conn = url.openConnection() as HttpsURLConnection
        conn.connectTimeout = 15_000
        conn.readTimeout = 30_000
        // Handle redirects (GitHub releases redirect)
        conn.instanceFollowRedirects = true
        conn.connect()

        val total = conn.contentLength.toLong()
        var downloaded = 0L

        conn.inputStream.use { input ->
            FileOutputStream(dest).use { output ->
                val buf = ByteArray(8192)
                var lastPercent = -1
                while (true) {
                    val n = input.read(buf)
                    if (n <= 0) break
                    output.write(buf, 0, n)
                    downloaded += n
                    if (total > 0) {
                        val percent = (downloaded * 100 / total).toInt()
                        if (percent != lastPercent) {
                            lastPercent = percent
                            onPercent(percent)
                        }
                    }
                }
            }
        }
    }

    private fun extractTarXz(archive: File, destDir: File, @Suppress("UNUSED_PARAMETER") context: Context) {
        // Pure Java extraction to avoid noexec restrictions on Android 10+
        // Strip top-level dir (e.g. archlinux-aarch64/) from the proot-distro archive
        FileInputStream(archive).use { fis ->
            XZInputStream(fis).use { xz ->
                BootstrapInstaller.extractTar(xz, destDir, stripComponents = 1)
            }
        }
    }

    private fun configurePacman(rootfsDir: File) {
        // Disable sandbox (Android kernels lack landlock)
        val pacmanConf = File(rootfsDir, "etc/pacman.conf")
        if (pacmanConf.exists()) {
            var content = pacmanConf.readText()
            if (!content.contains("DisableSandbox")) {
                content += "\nDisableSandbox\n"
                pacmanConf.writeText(content)
            }
        }

        // Set locale
        val localeGen = File(rootfsDir, "etc/locale.gen")
        if (localeGen.exists()) {
            val content = localeGen.readText()
            localeGen.writeText(
                content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8"),
            )
        }
    }
}
