// SPDX-License-Identifier: GPL-3.0-or-later
//! 🖹 en los materiales con tarifa: abre la tarifa del fabricante (de la
//! carpeta de tarifas) en la página donde aparece su referencia.

use super::*;
use ppto_biblioteca::{carpeta, indice};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc;

impl Aplicacion {
    /// Fabricantes con tarifa en la carpeta (nombres de carpeta), leídos
    /// como mucho cada 20 s.
    pub(super) fn fabricantes_con_tarifa(&mut self) -> BTreeSet<String> {
        let fresco = self
            .carpeta_tarifas
            .as_ref()
            .is_some_and(|(t, _)| t.elapsed() < std::time::Duration::from_secs(20));
        if !fresco {
            let v: BTreeSet<String> = carpeta::ruta_carpeta()
                .map(|r| carpeta::explorar(&r))
                .map(|c| {
                    c.fabricantes
                        .into_iter()
                        .map(|(f, _)| carpeta::nombre_carpeta(&f))
                        .collect()
                })
                .unwrap_or_default();
            self.carpeta_tarifas = Some((std::time::Instant::now(), v));
        }
        self.carpeta_tarifas
            .as_ref()
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    /// Busca la referencia en las tarifas del fabricante (en segundo plano:
    /// la primera vez se lee el PDF entero) y la abre en su página.
    pub(super) fn abrir_tarifa(&mut self, fabricante: String, referencia: Option<String>) {
        let Some(dir) = carpeta::ruta_carpeta() else {
            self.error("No hay carpeta de tarifas: elíjala en la ventana Biblioteca.");
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.abriendo_tarifa = Some(rx);
        self.info(format!(
            "Buscando {} en las tarifas de {fabricante}…",
            referencia.as_deref().unwrap_or("la referencia")
        ));
        std::thread::spawn(move || {
            let r = match &referencia {
                Some(r) => indice::buscar(&dir, &fabricante, r),
                None => carpeta::vigente(&dir, &fabricante).map(|doc| indice::Encontrada {
                    doc,
                    pagina: None,
                    avisos: Vec::new(),
                }),
            };
            let m = match r {
                None => Err(format!("No hay tarifa de {fabricante} en la carpeta de tarifas.")),
                Some(e) => {
                    abrir_pdf_en_pagina(&e.doc.ruta, e.pagina);
                    let nombre = e
                        .doc
                        .ruta
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    match (e.pagina, &referencia) {
                        (Some(p), Some(r)) => Ok(format!("{r}: página {p} de «{nombre}».")),
                        (None, Some(r)) => Err(format!(
                            "{r} no aparece en las tarifas PDF de {fabricante}: se abre «{nombre}» por el principio.{}",
                            if e.avisos.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", e.avisos.join("; "))
                            }
                        )),
                        _ => Ok(format!("Material sin referencia: se abre «{nombre}».")),
                    }
                }
            };
            let _ = tx.send(m);
        });
    }

    /// Mensaje de la búsqueda en segundo plano, cuando termina.
    pub(super) fn sondear_tarifa(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.abriendo_tarifa else { return };
        match rx.try_recv() {
            Ok(Ok(m)) => {
                self.abriendo_tarifa = None;
                self.info(m);
            }
            Ok(Err(m)) => {
                self.abriendo_tarifa = None;
                self.error(m);
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            Err(mpsc::TryRecvError::Disconnected) => self.abriendo_tarifa = None,
        }
    }
}

/// `file:///C:/Tarifas/S%26P/2026-06%20Tarifa.pdf#page=12`
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn url_de(ruta: &Path, pagina: Option<u32>) -> String {
    let s = ruta.display().to_string().replace('\\', "/");
    let mut u = String::from(if s.starts_with('/') { "file://" } else { "file:///" });
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '/' | ':' | '.' | '-' | '_' | '~' => u.push(c),
            c => {
                let mut b = [0; 4];
                for x in c.encode_utf8(&mut b).bytes() {
                    u.push_str(&format!("%{x:02X}"));
                }
            }
        }
    }
    if let Some(p) = pagina {
        u.push_str(&format!("#page={p}"));
    }
    u
}

/// Abre el PDF en la página: con Edge o Chrome (que respetan `#page=`); si no
/// están, o sin página, con el programa del sistema.
pub(super) fn abrir_pdf_en_pagina(ruta: &Path, pagina: Option<u32>) {
    if pagina.is_some() {
        #[cfg(target_os = "windows")]
        {
            let mut exes = Vec::new();
            for v in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
                if let Ok(b) = std::env::var(v) {
                    let b = std::path::PathBuf::from(b);
                    exes.push(b.join(r"Microsoft\Edge\Application\msedge.exe"));
                    exes.push(b.join(r"Google\Chrome\Application\chrome.exe"));
                }
            }
            if let Some(e) = exes.iter().find(|e| e.exists())
                && std::process::Command::new(e).arg(url_de(ruta, pagina)).spawn().is_ok()
            {
                return;
            }
        }
    }
    super::asistente::abrir_en_sistema(ruta);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_con_pagina() {
        assert_eq!(
            url_de(Path::new(r"C:\Tarifas\S&P\2026-06 Tarifa S&P.pdf"), Some(12)),
            "file:///C:/Tarifas/S%26P/2026-06%20Tarifa%20S%26P.pdf#page=12"
        );
        assert_eq!(url_de(Path::new("/t/a ñ.pdf"), None), "file:///t/a%20%C3%B1.pdf");
    }
}
