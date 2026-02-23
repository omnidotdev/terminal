package dev.omnidotdev.terminal

import android.content.Context
import android.system.Os
import java.io.File
import java.io.InputStream

object BootstrapInstaller {
    private const val BOOTSTRAP_VERSION = "1"
    // AGP decompresses .gz assets at build time, so the APK contains the raw tar
    private const val BOOTSTRAP_ASSET = "bootstrap-aarch64.tar"

    fun isInstalled(context: Context): Boolean {
        val filesDir = context.filesDir
        val versionFile = File(filesDir, "usr/.bootstrap_version")
        val busybox = File(filesDir, "usr/bin/busybox")
        return versionFile.exists()
            && busybox.exists()
            && versionFile.readText().trim() == BOOTSTRAP_VERSION
    }

    fun install(context: Context, onProgress: (String) -> Unit) {
        val filesDir = context.filesDir

        // Verify asset exists in APK before proceeding
        val available = context.assets.list("") ?: emptyArray()
        if (BOOTSTRAP_ASSET !in available) {
            throw IllegalStateException(
                "Asset '$BOOTSTRAP_ASSET' not found in APK " +
                    "(available: ${available.joinToString()}). Clean rebuild required."
            )
        }

        // Create directory structure
        onProgress("Creating directories...")
        val dirs = listOf("home", "usr/bin", "usr/tmp", "usr/etc")
        for (dir in dirs) {
            File(filesDir, dir).mkdirs()
        }

        // Extract tar from assets (AGP strips .gz at build time)
        onProgress("Extracting bootstrap archive...")
        context.assets.open(BOOTSTRAP_ASSET).use { raw ->
            extractTar(raw, filesDir)
        }

        // Make busybox executable
        val busybox = File(filesDir, "usr/bin/busybox")
        busybox.setExecutable(true, false)

        // Make setup-storage executable
        val setupStorage = File(filesDir, "usr/bin/setup-storage")
        if (setupStorage.exists()) {
            setupStorage.setExecutable(true, false)
        }

        // Create symlinks from busybox --list
        onProgress("Creating symlinks...")
        createBusyboxSymlinks(busybox)

        // Create default .profile
        onProgress("Writing shell profile...")
        writeProfile(File(filesDir, "home"))

        // Write version marker
        File(filesDir, "usr/.bootstrap_version").writeText(BOOTSTRAP_VERSION)

        onProgress("Done")
    }

    private fun createBusyboxSymlinks(busybox: File) {
        val binDir = busybox.parentFile ?: return
        val applets = queryBusyboxApplets(busybox)

        for (name in applets) {
            if (name.isEmpty() || name == "busybox") continue
            val link = File(binDir, name)
            if (!link.exists()) {
                try {
                    Os.symlink(busybox.absolutePath, link.absolutePath)
                } catch (_: Exception) {
                    // Symlink may already exist or name may conflict
                }
            }
        }
    }

    /** Query busybox for its applet list, falling back to a common set
     *  when execution is blocked (Android noexec on app data dirs). */
    private fun queryBusyboxApplets(busybox: File): List<String> {
        try {
            val process = ProcessBuilder(busybox.absolutePath, "--list")
                .redirectErrorStream(true)
                .start()
            val output = process.inputStream.bufferedReader().readText().trim()
            process.waitFor()
            if (output.isNotEmpty()) return output.lines().map { it.trim() }
        } catch (_: Exception) {
            // Execution blocked (noexec mount on Android 10+)
        }
        return FALLBACK_APPLETS
    }

    // Common busybox applets for android_ndk_defconfig + ash shell features
    private val FALLBACK_APPLETS = listOf(
        "ash", "sh",
        "cat", "chmod", "chown", "cp", "cut", "date", "dd", "df",
        "dirname", "du", "echo", "env", "expr", "false", "find",
        "grep", "egrep", "fgrep", "head", "id", "kill", "ln", "ls",
        "mkdir", "mktemp", "mv", "nice", "nohup", "od", "patch",
        "printf", "ps", "pwd", "readlink", "realpath", "rm", "rmdir",
        "sed", "seq", "sleep", "sort", "stat", "strings", "tail",
        "tar", "tee", "test", "touch", "tr", "true", "tty",
        "uname", "uniq", "wc", "which", "whoami", "xargs", "yes",
    )

    private fun writeProfile(homeDir: File) {
        val profile = File(homeDir, ".profile")
        if (profile.exists()) return

        profile.writeText(
            """
            |# Omni Terminal
            |alias ls='ls --color=auto'
            |alias ll='ls -la'
            |alias la='ls -A'
            |alias grep='grep --color=auto'
            |
            |PS1='$ '
            """.trimMargin() + "\n"
        )
    }

    /** Minimal POSIX tar extractor with optional prefix stripping and symlink support. */
    internal fun extractTar(input: InputStream, destDir: File, stripComponents: Int = 0) {
        val header = ByteArray(512)
        while (true) {
            val bytesRead = readFully(input, header)
            if (bytesRead < 512) break

            // Check for end-of-archive (two consecutive zero blocks)
            if (header.all { it == 0.toByte() }) break

            // Parse file name (bytes 0..99, null-terminated)
            val nameEnd = header.indexOf(0, 0, 100)
            var name = String(header, 0, if (nameEnd >= 0) nameEnd else 100).trim()
            if (name.isEmpty()) break

            // UStar prefix field (bytes 345..499) for long paths
            val prefixEnd = header.indexOf(0, 345, 500)
            val prefix = String(header, 345, if (prefixEnd >= 345) prefixEnd - 345 else 155).trim()
            if (prefix.isNotEmpty()) {
                name = "$prefix/$name"
            }

            // Strip leading path components
            var strippedName = name
            if (stripComponents > 0) {
                val parts = name.removePrefix("./").split("/")
                strippedName = if (parts.size > stripComponents) {
                    parts.drop(stripComponents).joinToString("/")
                } else {
                    "" // Entry is within the stripped prefix itself
                }
            }

            // Parse file mode (bytes 100..107, octal ASCII)
            val modeStr = String(header, 100, 7).trim()
            val mode = if (modeStr.isNotEmpty()) modeStr.toInt(8) else 0x1FF

            // Parse file size (bytes 124..135, octal ASCII)
            val sizeStr = String(header, 124, 11).trim()
            val size = if (sizeStr.isNotEmpty()) sizeStr.toLong(8) else 0L

            // Parse type flag (byte 156): '0'/'\0' = regular file, '2' = symlink, '5' = directory
            val typeFlag = header[156]

            // Parse symlink target (bytes 157..256)
            val linkEnd = header.indexOf(0, 157, 257)
            val linkTarget = String(header, 157, if (linkEnd >= 157) linkEnd - 157 else 100).trim()

            if (strippedName.isEmpty()) {
                // Stripped away entirely — skip data
                val blocks = (size + 511) / 512
                input.skip(blocks * 512)
                continue
            }

            if (typeFlag == '5'.code.toByte()) {
                File(destDir, strippedName).mkdirs()
            } else if (typeFlag == '2'.code.toByte() && linkTarget.isNotEmpty()) {
                // Symlink
                val linkFile = File(destDir, strippedName)
                linkFile.parentFile?.mkdirs()
                try {
                    Os.symlink(linkTarget, linkFile.absolutePath)
                } catch (_: Exception) {
                    // Symlink may already exist
                }
            } else if (typeFlag == '0'.code.toByte() || typeFlag == 0.toByte()) {
                val outFile = File(destDir, strippedName)
                outFile.parentFile?.mkdirs()
                outFile.outputStream().use { out ->
                    var remaining = size
                    val buf = ByteArray(4096)
                    while (remaining > 0) {
                        val toRead = minOf(remaining.toInt(), buf.size)
                        val n = input.read(buf, 0, toRead)
                        if (n <= 0) break
                        out.write(buf, 0, n)
                        remaining -= n
                    }
                }
                // Apply file permissions from tar header
                if (mode and 0x49 != 0) { // Any execute bit (owner/group/other)
                    outFile.setExecutable(true, false)
                }
                if (mode and 0x92 != 0) { // Any write bit
                    outFile.setWritable(true, false)
                }
                outFile.setReadable(true, false)
                val padding = (512 - (size % 512).toInt()) % 512
                input.skip(padding.toLong())
            } else {
                // Skip unknown entry data + padding
                val blocks = (size + 511) / 512
                input.skip(blocks * 512)
            }
        }
    }

    private fun readFully(input: InputStream, buf: ByteArray): Int {
        var offset = 0
        while (offset < buf.size) {
            val n = input.read(buf, offset, buf.size - offset)
            if (n <= 0) break
            offset += n
        }
        return offset
    }

    private fun ByteArray.indexOf(value: Byte, from: Int, to: Int): Int {
        for (i in from until minOf(to, size)) {
            if (this[i] == value) return i
        }
        return -1
    }
}
