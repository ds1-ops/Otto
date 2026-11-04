//! System clipboard. On Windows a graph goes on the clipboard as EMF (vector, for Office,
//! Illustrator, CorelDRAW), `image/svg+xml` (Office 365, Inkscape, Affinity, Figma) and
//! PNG (fallback), all at once; the pasting app picks the best format it understands.

use otto_plot::Rendered;

use crate::EngineError;

#[cfg(windows)]
mod imp {
    use windows::core::w;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::Graphics::Gdi::SetEnhMetaFileBits;
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::{CF_ENHMETAFILE, CF_UNICODETEXT};

    use crate::EngineError;

    fn err(e: impl std::fmt::Display) -> EngineError {
        EngineError::Clipboard(e.to_string())
    }

    /// Opens the clipboard (retrying briefly if another app holds it) and empties it.
    struct Open;

    impl Open {
        fn new() -> Result<Self, EngineError> {
            let mut last = None;
            for _ in 0..10 {
                match unsafe { OpenClipboard(None) } {
                    Ok(()) => {
                        unsafe { EmptyClipboard() }.map_err(err)?;
                        return Ok(Open);
                    }
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                }
            }
            Err(err(last.map(|e| e.to_string()).unwrap_or_default()))
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            let _ = unsafe { CloseClipboard() };
        }
    }

    fn global(bytes: &[u8]) -> Result<HGLOBAL, EngineError> {
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)).map_err(err)?;
            let p = GlobalLock(h) as *mut u8;
            if p.is_null() {
                return Err(err("GlobalLock failed"));
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            let _ = GlobalUnlock(h);
            Ok(h)
        }
    }

    fn put(format: u32, bytes: &[u8]) -> Result<(), EngineError> {
        let h = global(bytes)?;
        unsafe { SetClipboardData(format, Some(HANDLE(h.0))) }.map_err(err)?;
        Ok(())
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect()
    }

    pub fn set_text(text: &str) -> Result<(), EngineError> {
        let _open = Open::new()?;
        put(CF_UNICODETEXT.0 as u32, &utf16(text))
    }

    pub fn set_graph(emf: &[u8], svg: &str, png: &[u8]) -> Result<(), EngineError> {
        let _open = Open::new()?;
        unsafe {
            let hemf = SetEnhMetaFileBits(emf);
            if hemf.is_invalid() {
                return Err(err("invalid EMF"));
            }
            SetClipboardData(CF_ENHMETAFILE.0 as u32, Some(HANDLE(hemf.0))).map_err(err)?;
            let svg_format = RegisterClipboardFormatW(w!("image/svg+xml"));
            put(svg_format, svg.as_bytes())?;
            let png_format = RegisterClipboardFormatW(w!("PNG"));
            put(png_format, png)?;
        }
        Ok(())
    }
}

/// Puts plain text (e.g. tab-separated data or R code) on the clipboard.
pub fn set_text(text: &str) -> Result<(), EngineError> {
    #[cfg(windows)]
    {
        imp::set_text(text)
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err(EngineError::Clipboard("clipboard access is only implemented on Windows so far".into()))
    }
}

/// Puts a graph on the clipboard as vector graphics.
pub fn set_graph(r: &Rendered) -> Result<(), EngineError> {
    #[cfg(windows)]
    {
        imp::set_graph(&r.emf()?, &r.svg(), &r.png(600.0)?)
    }
    #[cfg(not(windows))]
    {
        let _ = r;
        Err(EngineError::Clipboard("vector clipboard export is only implemented on Windows so far".into()))
    }
}
