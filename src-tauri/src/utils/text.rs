/// Decode a chunk of bytes from a child process's stdout/stderr into text.
///
/// The bundled yt-dlp / gallery-dl sidecars are PyInstaller-frozen Python
/// programs. When their output is piped (as it is here), CPython encodes text
/// using the Windows ANSI code page (cp1252 on Western installs), so a path
/// like `Téléchargements` is emitted with `é` as the single byte `0xE9` — which
/// is invalid UTF-8. Decoding that with `String::from_utf8_lossy` corrupts it
/// into `T�l�chargements`, which then never matches the real file on disk.
///
/// So: decode as UTF-8 first (correct for system binaries and any UTF-8 output),
/// and only on Windows fall back to Windows-1252 when the bytes aren't valid
/// UTF-8, which recovers accented characters exactly.
pub fn decode_process_output(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            #[cfg(target_os = "windows")]
            {
                encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned()
            }
            #[cfg(not(target_os = "windows"))]
            {
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    }
}
