//! Terminal image protocol detection for `--visual` output.
//!
//! Detects Sixel or iTerm2 inline image protocol support based on
//! environment variables, with CLI flag overrides.

/// Supported image protocols for terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Sixel,
    Iterm2,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Sixel => write!(f, "Sixel"),
            Protocol::Iterm2 => write!(f, "iTerm2"),
        }
    }
}

/// Error when no protocol can be detected.
#[derive(Debug, Clone)]
pub struct NoProtocolDetected;

impl std::fmt::Display for NoProtocolDetected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "No supported image protocol detected. Use --sixel or --iterm2 to override, or use --sparkline instead."
        )
    }
}

/// Detect the terminal image protocol.
///
/// Priority:
/// 1. `force_sixel` flag → Sixel
/// 2. `force_iterm2` flag → iTerm2
/// 3. `TERM_PROGRAM == "WezTerm"` → Sixel
/// 4. `TERM_PROGRAM == "iTerm.app"` → iTerm2
/// 5. `TERM` contains "sixel" → Sixel
/// 6. None detected → error
pub fn detect_protocol(
    force_sixel: bool,
    force_iterm2: bool,
) -> Result<Protocol, NoProtocolDetected> {
    detect_protocol_with_env(force_sixel, force_iterm2, |key| std::env::var(key))
}

/// Testable version that takes an environment variable lookup function.
fn detect_protocol_with_env(
    force_sixel: bool,
    force_iterm2: bool,
    env_var: impl Fn(&str) -> Result<String, std::env::VarError>,
) -> Result<Protocol, NoProtocolDetected>
{
    // Flag overrides
    if force_sixel {
        return Ok(Protocol::Sixel);
    }
    if force_iterm2 {
        return Ok(Protocol::Iterm2);
    }

    // TERM_PROGRAM check
    if let Ok(term_program) = env_var("TERM_PROGRAM") {
        if term_program == "WezTerm" {
            return Ok(Protocol::Sixel);
        }
        if term_program == "iTerm.app" {
            return Ok(Protocol::Iterm2);
        }
    }

    // TERM check for sixel
    if let Ok(term) = env_var("TERM") {
        if term.to_lowercase().contains("sixel") {
            return Ok(Protocol::Sixel);
        }
    }

    Err(NoProtocolDetected)
}

/// Display a PNG image in the terminal using the specified protocol.
pub fn display_image(png_data: &[u8], protocol: Protocol) -> Result<(), String> {
    match protocol {
        Protocol::Iterm2 => display_iterm2(png_data),
        Protocol::Sixel => display_sixel(png_data),
    }
}

/// Display image using iTerm2 inline image protocol.
/// Format: ESC ] 1337 ; File=inline=1;size=<size>:<base64> BEL
fn display_iterm2(png_data: &[u8]) -> Result<(), String> {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png_data);
    print!(
        "\x1b]1337;File=inline=1;size={size};width=100%:{data}\x07",
        size = png_data.len(),
        data = encoded,
    );
    Ok(())
}

/// Get the terminal pixel width for Sixel scaling.
/// Falls back to the original image width if detection fails.
fn terminal_pixel_width() -> Option<u32> {
    // Try TIOCGWINSZ ioctl to get pixel size
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        #[repr(C)]
        struct Winsize {
            ws_row: u16,
            ws_col: u16,
            ws_xpixel: u16,
            ws_ypixel: u16,
        }
        let mut ws = MaybeUninit::<Winsize>::uninit();
        // TIOCGWINSZ = 0x5413 on Linux, 0x40087468 on macOS
        #[cfg(target_os = "macos")]
        const TIOCGWINSZ: libc::c_ulong = 0x40087468;
        #[cfg(target_os = "linux")]
        const TIOCGWINSZ: libc::c_ulong = 0x5413;
        let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, TIOCGWINSZ, ws.as_mut_ptr()) };
        if ret == 0 {
            let ws = unsafe { ws.assume_init() };
            if ws.ws_xpixel > 0 {
                return Some(ws.ws_xpixel as u32);
            }
        }
    }
    None
}

/// Display image using Sixel protocol.
/// Converts PNG to RGBA, quantizes to 256 colors, and encodes as Sixel.
/// Scales image to fit terminal width while preserving aspect ratio.
fn display_sixel(png_data: &[u8]) -> Result<(), String> {
    let img = image::load_from_memory(png_data)
        .map_err(|e| format!("Failed to decode PNG: {e}"))?;

    // Scale to terminal width if possible
    let img = if let Some(term_px_width) = terminal_pixel_width() {
        let (w, h) = (img.width(), img.height());
        if term_px_width < w {
            let new_h = (h as f64 * term_px_width as f64 / w as f64) as u32;
            img.resize_exact(term_px_width, new_h, image::imageops::FilterType::Lanczos3)
        } else {
            img
        }
    } else {
        img
    };

    let img = img.to_rgba8();
    let (width, height) = img.dimensions();

    // Build palette: collect unique colors (up to 256)
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut pixel_indices: Vec<u16> = Vec::with_capacity((width * height) as usize);

    for pixel in img.pixels() {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        let alpha = pixel[3];

        // Transparent pixels → black (index 0 will be set to black)
        let color = if alpha < 128 { [0, 0, 0] } else { rgb };

        let idx = if let Some(pos) = palette.iter().position(|c| *c == color) {
            pos as u16
        } else if palette.len() < 256 {
            palette.push(color);
            (palette.len() - 1) as u16
        } else {
            // Find nearest color
            nearest_color(&palette, &color) as u16
        };
        pixel_indices.push(idx);
    }

    // Sixel output
    let mut out = String::new();

    // DCS q - start sixel
    out.push_str("\x1bPq\n");

    // Define palette
    for (i, color) in palette.iter().enumerate() {
        let r = (color[0] as u32 * 100) / 255;
        let g = (color[1] as u32 * 100) / 255;
        let b = (color[2] as u32 * 100) / 255;
        out.push_str(&format!("#{i};2;{r};{g};{b}"));
    }
    out.push('\n');

    // Sixel data: process 6 rows at a time
    let mut row = 0;
    while row < height {
        let band_height = 6.min(height - row);

        for color_idx in 0..palette.len() {
            let mut has_pixels = false;
            let mut sixel_row = Vec::with_capacity(width as usize);

            for x in 0..width {
                let mut sixel_val: u8 = 0;
                for dy in 0..band_height {
                    let y = row + dy;
                    let px_idx = (y * width + x) as usize;
                    if pixel_indices[px_idx] == color_idx as u16 {
                        sixel_val |= 1 << dy;
                        has_pixels = true;
                    }
                }
                sixel_row.push(sixel_val + 0x3f);
            }

            if has_pixels {
                out.push('#');
                out.push_str(&color_idx.to_string());
                for &ch in &sixel_row {
                    out.push(ch as char);
                }
                // Use $ (carriage return) for all colors except the last one in this band
                out.push('$');
                out.push('\n');
            }
        }

        // Move to next band with -
        out.push('-');
        out.push('\n');

        row += 6;
    }

    // ST - end sixel
    out.push_str("\x1b\\");

    print!("{out}");
    Ok(())
}

fn nearest_color(palette: &[[u8; 3]], target: &[u8; 3]) -> usize {
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, c)| {
            let dr = c[0] as i32 - target[0] as i32;
            let dg = c[1] as i32 - target[1] as i32;
            let db = c[2] as i32 - target[2] as i32;
            dr * dr + dg * dg + db * db
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::VarError;

    /// Helper: create an env lookup from a list of (key, value) pairs.
    fn mock_env(vars: Vec<(&'static str, &'static str)>) -> impl Fn(&str) -> Result<String, VarError> {
        move |key: &str| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
                .ok_or(VarError::NotPresent)
        }
    }

    // --- Flag overrides ---

    #[test]
    fn test_force_sixel() {
        let env = mock_env(vec![]);
        let result = detect_protocol_with_env(true, false, env);
        assert_eq!(result.unwrap(), Protocol::Sixel);
    }

    #[test]
    fn test_force_iterm2() {
        let env = mock_env(vec![]);
        let result = detect_protocol_with_env(false, true, env);
        assert_eq!(result.unwrap(), Protocol::Iterm2);
    }

    #[test]
    fn test_force_sixel_overrides_iterm2_env() {
        let env = mock_env(vec![("TERM_PROGRAM", "iTerm.app")]);
        let result = detect_protocol_with_env(true, false, env);
        assert_eq!(result.unwrap(), Protocol::Sixel);
    }

    #[test]
    fn test_sixel_flag_takes_priority_over_iterm2_flag() {
        let env = mock_env(vec![]);
        let result = detect_protocol_with_env(true, true, env);
        assert_eq!(result.unwrap(), Protocol::Sixel);
    }

    // --- TERM_PROGRAM detection ---

    #[test]
    fn test_wezterm_detected_as_sixel() {
        let env = mock_env(vec![("TERM_PROGRAM", "WezTerm")]);
        let result = detect_protocol_with_env(false, false, env);
        assert_eq!(result.unwrap(), Protocol::Sixel);
    }

    #[test]
    fn test_iterm_detected() {
        let env = mock_env(vec![("TERM_PROGRAM", "iTerm.app")]);
        let result = detect_protocol_with_env(false, false, env);
        assert_eq!(result.unwrap(), Protocol::Iterm2);
    }

    #[test]
    fn test_unknown_term_program() {
        let env = mock_env(vec![("TERM_PROGRAM", "Alacritty")]);
        let result = detect_protocol_with_env(false, false, env);
        assert!(result.is_err());
    }

    // --- TERM fallback ---

    #[test]
    fn test_term_sixel_keyword() {
        let env = mock_env(vec![("TERM", "xterm-sixel")]);
        let result = detect_protocol_with_env(false, false, env);
        assert_eq!(result.unwrap(), Protocol::Sixel);
    }

    #[test]
    fn test_term_sixel_case_insensitive() {
        let env = mock_env(vec![("TERM", "XTERM-SIXEL")]);
        let result = detect_protocol_with_env(false, false, env);
        assert_eq!(result.unwrap(), Protocol::Sixel);
    }

    #[test]
    fn test_term_no_sixel() {
        let env = mock_env(vec![("TERM", "xterm-256color")]);
        let result = detect_protocol_with_env(false, false, env);
        assert!(result.is_err());
    }

    // --- No detection ---

    #[test]
    fn test_no_env_vars_at_all() {
        let env = mock_env(vec![]);
        let result = detect_protocol_with_env(false, false, env);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_message_suggests_alternatives() {
        let env = mock_env(vec![]);
        let err = detect_protocol_with_env(false, false, env).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("--sixel"));
        assert!(msg.contains("--iterm2"));
        assert!(msg.contains("--sparkline"));
    }

    // --- TERM_PROGRAM takes priority over TERM ---

    #[test]
    fn test_term_program_priority_over_term() {
        let env = mock_env(vec![
            ("TERM_PROGRAM", "iTerm.app"),
            ("TERM", "xterm-sixel"),
        ]);
        let result = detect_protocol_with_env(false, false, env);
        assert_eq!(result.unwrap(), Protocol::Iterm2);
    }

    // --- Display ---

    #[test]
    fn test_protocol_display() {
        assert_eq!(Protocol::Sixel.to_string(), "Sixel");
        assert_eq!(Protocol::Iterm2.to_string(), "iTerm2");
    }
}
