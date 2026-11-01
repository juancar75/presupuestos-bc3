// SPDX-License-Identifier: GPL-3.0-or-later
//! Actualización del programa desde las publicaciones del repositorio.
//!
//! 1. [`buscar`] consulta la lista de versiones de GitHub (una petición
//!    anónima a la API pública, sin datos del usuario ni de sus obras) y
//!    devuelve la más reciente si es posterior a la instalada.
//! 2. [`descargar`] baja el zip de Windows a una carpeta temporal y lo
//!    extrae (solo ficheros normales, sin salirse de la carpeta).
//! 3. [`instalar`] lanza su `instalar.ps1`, que cierra el programa, copia
//!    los ficheros y lo vuelve a abrir.

use serde_json::Value;
use std::cmp::Ordering;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as Atom};

pub const REPOSITORIO: &str = "juancar75/presupuestos-bc3";

/// Una versión publicada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// «0.1.0-alpha.37» (sin la «v»).
    pub numero: String,
    pub notas: String,
    /// Página de la versión.
    pub pagina: String,
    /// Zip para Windows (`…-windows.zip`).
    pub zip: Option<String>,
    pub tamano: u64,
}

/// Compara versiones «0.1.0-alpha.36» / «v0.1.0»: número principal y luego
/// la parte de prueba (una versión sin «-alpha» va después de sus alfas).
pub fn comparar(a: &str, b: &str) -> Ordering {
    fn partes(v: &str) -> (Vec<u64>, Option<Vec<u64>>) {
        let v = v.trim().trim_start_matches(['v', 'V']);
        let (base, pre) = match v.split_once('-') {
            Some((b, p)) => (b, Some(p)),
            None => (v, None),
        };
        let nums = |s: &str| -> Vec<u64> {
            s.split(|c: char| !c.is_ascii_digit())
                .filter(|x| !x.is_empty())
                .filter_map(|x| x.parse().ok())
                .collect()
        };
        (nums(base), pre.map(nums))
    }
    let (ba, pa) = partes(a);
    let (bb, pb) = partes(b);
    ba.cmp(&bb).then_with(|| match (pa, pb) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => x.cmp(&y),
    })
}

/// La versión más reciente de la lista de la API (`/releases`), sin
/// borradores.
pub fn ultima(json: &str) -> Option<Version> {
    let v: Value = serde_json::from_str(json).ok()?;
    let mut lista: Vec<Version> = v
        .as_array()?
        .iter()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let numero = r["tag_name"].as_str()?.trim_start_matches(['v', 'V']).to_owned();
            let zip = r["assets"].as_array().and_then(|a| {
                a.iter()
                    .find(|x| x["name"].as_str().is_some_and(|n| n.ends_with("-windows.zip")))
                    .map(|x| {
                        (
                            x["browser_download_url"].as_str().unwrap_or_default().to_owned(),
                            x["size"].as_u64().unwrap_or_default(),
                        )
                    })
            });
            Some(Version {
                numero,
                notas: r["body"].as_str().unwrap_or_default().to_owned(),
                pagina: r["html_url"].as_str().unwrap_or_default().to_owned(),
                tamano: zip.as_ref().map(|z| z.1).unwrap_or_default(),
                zip: zip.map(|z| z.0).filter(|u| !u.is_empty()),
            })
        })
        .collect();
    lista.sort_by(|a, b| comparar(&b.numero, &a.numero));
    lista.into_iter().next()
}

fn agente() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(900)))
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .user_agent(concat!("presupuestos-bc3/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Versión publicada posterior a `actual`, si la hay (con su zip).
pub fn buscar(actual: &str) -> Result<Option<Version>, String> {
    let url = format!("https://api.github.com/repos/{REPOSITORIO}/releases?per_page=10");
    let texto = agente()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("no se pudo consultar GitHub: {e}"))?
        .body_mut()
        .with_config()
        .limit(10 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(ultima(&texto).filter(|v| comparar(&v.numero, actual) == Ordering::Greater && v.zip.is_some()))
}

/// Descarga y extrae el zip en `carpeta` (se vacía antes). Devuelve la
/// ruta de `instalar.ps1`. `progreso` cuenta los bytes descargados.
pub fn descargar(v: &Version, carpeta: &Path, progreso: Arc<AtomicU64>) -> Result<PathBuf, String> {
    let url = v.zip.as_deref().ok_or("la versión no tiene zip para Windows")?;
    let _ = std::fs::remove_dir_all(carpeta);
    std::fs::create_dir_all(carpeta).map_err(|e| e.to_string())?;
    let mut r = agente()
        .get(url)
        .call()
        .map_err(|e| format!("no se pudo descargar: {e}"))?;
    let mut lector = r.body_mut().with_config().limit(1024 * 1024 * 1024).reader();
    let mut bytes = Vec::with_capacity(usize::try_from(v.tamano).unwrap_or(0));
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = lector
            .read(&mut buf)
            .map_err(|e| format!("descarga interrumpida: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        progreso.fetch_add(n as u64, Atom::Relaxed);
    }
    if v.tamano > 0 && bytes.len() as u64 != v.tamano {
        return Err(format!("descarga incompleta ({} de {} bytes)", bytes.len(), v.tamano));
    }
    extraer(&bytes, carpeta)?;
    buscar_instalador(carpeta).ok_or_else(|| "el zip no trae instalar.ps1".into())
}

/// Extrae solo ficheros normales y sin salirse de `carpeta`.
pub fn extraer(zip_bytes: &[u8], carpeta: &Path) -> Result<(), String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(|e| format!("zip no válido: {e}"))?;
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        if f.is_symlink() {
            continue;
        }
        let destino = carpeta.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&destino).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = destino.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut salida = std::fs::File::create(&destino).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut salida).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn buscar_instalador(carpeta: &Path) -> Option<PathBuf> {
    let directo = carpeta.join("instalar.ps1");
    if directo.is_file() {
        return Some(directo);
    }
    std::fs::read_dir(carpeta)
        .ok()?
        .flatten()
        .map(|e| e.path().join("instalar.ps1"))
        .find(|p| p.is_file())
}

/// Lanza el instalador sin esperar; cierra este programa y lo vuelve a abrir.
pub fn instalar(ps1: &Path) -> Result<(), String> {
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(ps1)
        .arg("-Abrir")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no se pudo lanzar el instalador: {e}"))
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn compara_versiones() {
        assert_eq!(comparar("0.1.0-alpha.37", "0.1.0-alpha.36"), Ordering::Greater);
        assert_eq!(comparar("v0.1.0-alpha.9", "0.1.0-alpha.10"), Ordering::Less);
        assert_eq!(comparar("0.1.0", "0.1.0-alpha.99"), Ordering::Greater);
        assert_eq!(comparar("0.2.0-alpha.1", "0.1.0"), Ordering::Greater);
        assert_eq!(comparar("0.1.0-alpha.36", "v0.1.0-alpha.36"), Ordering::Equal);
    }

    #[test]
    fn lee_la_lista_de_versiones() {
        let json = r#"[
          {"tag_name":"v0.1.0-alpha.36","draft":false,"body":"a","html_url":"h36",
           "assets":[{"name":"presupuestos-bc3-v0.1.0-alpha.36-windows.zip","browser_download_url":"u36","size":10}]},
          {"tag_name":"v0.1.0-alpha.38","draft":true,"body":"","html_url":"h38","assets":[]},
          {"tag_name":"v0.1.0-alpha.37","draft":false,"body":"Novedades","html_url":"h37",
           "assets":[{"name":"ppto.exe","browser_download_url":"x","size":1},
                     {"name":"presupuestos-bc3-v0.1.0-alpha.37-windows.zip","browser_download_url":"u37","size":70}]}
        ]"#;
        let v = ultima(json).unwrap();
        assert_eq!(v.numero, "0.1.0-alpha.37", "el borrador no cuenta");
        assert_eq!(
            (v.zip.as_deref(), v.tamano, v.notas.as_str()),
            (Some("u37"), 70, "Novedades")
        );
        assert!(ultima("no es json").is_none());
    }

    #[test]
    fn extrae_sin_salirse_de_la_carpeta() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            w.add_directory("paquete/", o).unwrap();
            w.start_file("paquete/instalar.ps1", o).unwrap();
            w.write_all(b"Write-Host hola").unwrap();
            w.start_file("../fuera.txt", o).unwrap();
            w.write_all(b"no").unwrap();
            w.finish().unwrap();
        }
        let dir = std::env::temp_dir().join(format!("ppto-actualizar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        extraer(buf.get_ref(), &dir).unwrap();
        assert!(dir.join("paquete/instalar.ps1").is_file());
        assert!(!dir.parent().unwrap().join("fuera.txt").exists());
        assert_eq!(buscar_instalador(&dir), Some(dir.join("paquete/instalar.ps1")));
        assert!(extraer(b"no es un zip", &dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
