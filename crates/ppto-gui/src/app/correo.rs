// SPDX-License-Identifier: GPL-3.0-or-later
//! Correos preparados para revisar y enviar desde **Outlook**.
//!
//! En Windows, con Outlook de escritorio, cada correo se abre como un
//! mensaje nuevo (destinatario, asunto, texto antes de la firma y el
//! adjunto) sin enviarse: lo envía el técnico. Si Outlook no responde, o
//! fuera de Windows, se crea un borrador `.eml` (cabecera `X-Unsent`) por
//! correo y se abre con el programa de correo del sistema.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Correo {
    pub para: String,
    pub asunto: String,
    pub cuerpo: String,
    pub adjunto: PathBuf,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(super) fn base64(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for t in b.chunks(3) {
        let n =
            (u32::from(t[0]) << 16) | (u32::from(*t.get(1).unwrap_or(&0)) << 8) | u32::from(*t.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= t.len() {
                s.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// Base64 en líneas de 76 caracteres (correo).
fn base64_lineas(b: &[u8]) -> String {
    let s = base64(b);
    s.as_bytes()
        .chunks(76)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Texto del correo en HTML (para ponerlo delante de la firma de Outlook).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(super) fn html(cuerpo: &str) -> String {
    let mut s = String::from("<div style=\"font-family:Calibri,Arial,sans-serif;font-size:11pt\">");
    for c in cuerpo.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            '\n' => s.push_str("<br>"),
            '\r' => {}
            c => s.push(c),
        }
    }
    s.push_str("<br></div>");
    s
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// Borrador `.eml` sin enviar con el adjunto.
pub(super) fn eml(c: &Correo) -> std::io::Result<Vec<u8>> {
    let adj = std::fs::read(&c.adjunto)?;
    let nombre = c
        .adjunto
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "adjunto.xlsx".into());
    let ascii: String = nombre
        .chars()
        .map(|x| if x.is_ascii() && x != '"' { x } else { '_' })
        .collect();
    let limite = "ppto-bc3-limite-7d1c";
    let mut s = String::new();
    s.push_str("X-Unsent: 1\r\n");
    s.push_str(&format!("To: {}\r\n", c.para.trim()));
    s.push_str(&format!("Subject: =?UTF-8?B?{}?=\r\n", base64(c.asunto.as_bytes())));
    s.push_str("MIME-Version: 1.0\r\n");
    s.push_str(&format!("Content-Type: multipart/mixed; boundary=\"{limite}\"\r\n\r\n"));
    s.push_str(&format!("--{limite}\r\n"));
    s.push_str("Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n");
    s.push_str(&base64_lineas(c.cuerpo.replace('\n', "\r\n").as_bytes()));
    s.push_str(&format!("\r\n--{limite}\r\n"));
    s.push_str(&format!(
        "Content-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet; name=\"{ascii}\"\r\n"
    ));
    s.push_str(&format!(
        "Content-Disposition: attachment; filename=\"{ascii}\"; filename*=UTF-8''{}\r\n",
        pct(&nombre)
    ));
    s.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
    s.push_str(&base64_lineas(&adj));
    s.push_str(&format!("\r\n--{limite}--\r\n"));
    Ok(s.into_bytes())
}

/// Crea un `.eml` por correo en `carpeta` y los abre con el sistema.
fn borradores(correos: &[Correo], carpeta: &Path) -> Result<String, String> {
    for c in correos {
        let r = c.adjunto.with_extension("eml");
        let r = carpeta.join(r.file_name().unwrap_or_default());
        let b = eml(c).map_err(|e| format!("No se pudo preparar el correo: {e}"))?;
        std::fs::write(&r, b).map_err(|e| format!("No se pudo guardar {}: {e}", r.display()))?;
        #[cfg(not(test))]
        super::asistente::abrir_en_sistema(&r);
    }
    Ok(format!(
        "{} borrador(es) de correo abierto(s) con el programa de correo del sistema.",
        correos.len()
    ))
}

/// Script de PowerShell (solo ASCII) que abre cada correo en Outlook.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const SCRIPT: &str = r#"param([string]$Lista)
$ErrorActionPreference = 'Stop'
$o = New-Object -ComObject Outlook.Application
$correos = Get-Content -Raw -Encoding UTF8 -LiteralPath $Lista | ConvertFrom-Json
foreach ($c in $correos) {
  $m = $o.CreateItem(0)
  $m.To = $c.para
  $m.Subject = $c.asunto
  if ($c.adjunto) { [void]$m.Attachments.Add([string]$c.adjunto) }
  $m.Display()
  $f = $m.HTMLBody
  $b = $f.IndexOf('<body', [StringComparison]::OrdinalIgnoreCase)
  if ($b -ge 0) {
    $i = $f.IndexOf('>', $b)
    $m.HTMLBody = $f.Substring(0, $i + 1) + $c.html + $f.Substring($i + 1)
  } else {
    $m.HTMLBody = $c.html + $f
  }
}
"#;

#[cfg(target_os = "windows")]
fn outlook(correos: &[Correo], carpeta: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let lista: Vec<_> = correos
        .iter()
        .map(|c| {
            serde_json::json!({
                "para": c.para.trim(),
                "asunto": c.asunto,
                "html": html(&c.cuerpo),
                "adjunto": c.adjunto.display().to_string(),
            })
        })
        .collect();
    let json = carpeta.join("correos.json");
    let ps1 = carpeta.join("abrir-en-outlook.ps1");
    std::fs::write(&json, serde_json::to_vec(&lista).unwrap_or_default()).map_err(|e| e.to_string())?;
    std::fs::write(&ps1, SCRIPT).map_err(|e| e.to_string())?;
    let s = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&ps1)
        .arg("-Lista")
        .arg(&json)
        .creation_flags(0x0800_0000) // sin ventana de consola
        .output()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&json);
    if s.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&s.stderr).trim().to_owned())
    }
}

#[cfg(not(target_os = "windows"))]
fn outlook(_: &[Correo], _: &Path) -> Result<(), String> {
    Err("Outlook solo está en Windows".into())
}

/// Abre los correos en Outlook (o como borradores `.eml`). `carpeta` es
/// donde dejar los ficheros de trabajo.
pub(super) fn preparar(correos: &[Correo], carpeta: &Path) -> Result<String, String> {
    if correos.is_empty() {
        return Err("No hay correos que preparar.".into());
    }
    match outlook(correos, carpeta) {
        Ok(()) => Ok(format!(
            "{} correo(s) abierto(s) en Outlook: revíselos y pulse Enviar.",
            correos.len()
        )),
        Err(_) => borradores(correos, carpeta),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn base64_como_el_estandar() {
        for (a, b) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
        ] {
            assert_eq!(base64(a.as_bytes()), b);
        }
        assert_eq!(base64("Petición".as_bytes()), "UGV0aWNpw7Nu");
    }

    #[test]
    fn borrador_eml_con_adjunto_y_sin_enviar() {
        let dir = std::env::temp_dir().join(format!("ppto-correo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let adj = dir.join("Petición S&P.xlsx");
        std::fs::write(&adj, b"PK\x03\x04datos").unwrap();
        let c = Correo {
            para: "ofertas@ejemplo.test".into(),
            asunto: "Petición de precios — Obra".into(),
            cuerpo: "Buenos días:\n<lista>".into(),
            adjunto: adj,
        };
        let t = String::from_utf8(eml(&c).unwrap()).unwrap();
        assert!(t.starts_with("X-Unsent: 1\r\nTo: ofertas@ejemplo.test\r\n"));
        assert!(t.contains(&base64(b"PK\x03\x04datos")));
        assert!(t.contains("filename*=UTF-8''Petici%C3%B3n%20S%26P.xlsx"));
        assert_eq!(
            html("a<b>\nc & d"),
            "<div style=\"font-family:Calibri,Arial,sans-serif;font-size:11pt\">a&lt;b&gt;<br>c &amp; d<br></div>"
        );
        assert!(SCRIPT.is_ascii());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
