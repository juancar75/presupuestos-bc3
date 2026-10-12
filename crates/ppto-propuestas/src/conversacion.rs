// SPDX-License-Identifier: GPL-3.0-or-later
//! Conversación con el asistente, por obra (`asistente.json`).
//!
//! El usuario escribe en el panel del programa (con ficheros adjuntos, p. ej.
//! la oferta de un fabricante) y el asistente responde. Hay dos maneras de
//! que responda:
//!
//! - **Encargo por MCP**: el mensaje queda pendiente y la IA del usuario
//!   (Claude Desktop u otra) lo lee con `leer_mensajes` y contesta con
//!   `responder`.
//! - **Directo**: el programa llama a la API con la clave del usuario.
//!
//! En los dos casos la IA no cambia el presupuesto: hace propuestas que el
//! usuario acepta o rechaza, o guarda informes.

use crate::{ahora, carpeta_config, con_bloqueo, escribir, leer_estricto};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn ruta_conversaciones() -> Option<PathBuf> {
    carpeta_config().map(|c| c.join("asistente.json"))
}

/// Carpeta donde el asistente guarda los informes de una obra.
pub fn carpeta_informes(obra: &str) -> Option<PathBuf> {
    carpeta_config().map(|c| c.join("informes").join(nombre_fichero(obra)))
}

/// Texto apto para nombre de fichero (sin barras ni caracteres reservados).
pub fn nombre_fichero(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let t = t.trim().trim_matches('.').to_owned();
    if t.is_empty() { "sin_nombre".into() } else { t.chars().take(80).collect() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Autor {
    Usuario,
    Asistente,
    /// Avisos del programa (errores de la API, límite de gasto…).
    Sistema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mensaje {
    pub id: u64,
    pub obra: String,
    pub autor: Autor,
    pub texto: String,
    /// Rutas de ficheros adjuntos (en el equipo del usuario).
    #[serde(default)]
    pub adjuntos: Vec<String>,
    /// Segundos desde 1970.
    pub creado: u64,
    /// Mensaje del usuario ya atendido por el asistente.
    #[serde(default)]
    pub atendido: bool,
    /// Coste de la respuesta en el modo directo (texto, p. ej. «0,0123 $»).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coste: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Conversaciones {
    pub siguiente: u64,
    pub mensajes: Vec<Mensaje>,
}

impl Conversaciones {
    /// Carga el fichero; si no existe o no se puede leer, vacío (solo lectura).
    pub fn cargar(ruta: &Path) -> Self {
        leer_estricto(ruta).unwrap_or_default()
    }

    /// Lee, modifica y guarda con el fichero bloqueado.
    pub fn modificar<R>(ruta: &Path, f: impl FnOnce(&mut Self) -> R) -> std::io::Result<R> {
        con_bloqueo(ruta, || {
            let mut c: Self = leer_estricto(ruta)?;
            let r = f(&mut c);
            let t = serde_json::to_string_pretty(&c).map_err(std::io::Error::other)?;
            escribir(ruta, &t)?;
            Ok(r)
        })
    }

    /// Añade un mensaje y devuelve su número.
    pub fn anadir(&mut self, obra: &str, autor: Autor, texto: &str, adjuntos: Vec<String>) -> u64 {
        self.siguiente = self
            .siguiente
            .max(self.mensajes.iter().map(|m| m.id).max().unwrap_or(0))
            + 1;
        self.mensajes.push(Mensaje {
            id: self.siguiente,
            obra: obra.to_owned(),
            autor,
            texto: texto.to_owned(),
            adjuntos,
            creado: ahora(),
            atendido: autor != Autor::Usuario,
            coste: None,
        });
        self.siguiente
    }

    pub fn de_obra(&self, obra: &str) -> Vec<&Mensaje> {
        self.mensajes.iter().filter(|m| m.obra == obra).collect()
    }

    /// Mensajes del usuario que el asistente aún no ha atendido.
    pub fn pendientes(&self, obra: &str) -> Vec<&Mensaje> {
        self.mensajes
            .iter()
            .filter(|m| m.obra == obra && m.autor == Autor::Usuario && !m.atendido)
            .collect()
    }

    /// Respuesta del asistente: marca como atendidos los mensajes pendientes
    /// de la obra (o solo los indicados) y añade la respuesta.
    pub fn responder(&mut self, obra: &str, a: &[u64], texto: &str, coste: Option<String>) -> u64 {
        for m in self
            .mensajes
            .iter_mut()
            .filter(|m| m.obra == obra && m.autor == Autor::Usuario && !m.atendido)
        {
            if a.is_empty() || a.contains(&m.id) {
                m.atendido = true;
            }
        }
        let id = self.anadir(obra, Autor::Asistente, texto, Vec::new());
        if let Some(m) = self.mensajes.last_mut() {
            m.coste = coste;
        }
        id
    }

    /// Borra la conversación de una obra.
    pub fn vaciar(&mut self, obra: &str) {
        self.mensajes.retain(|m| m.obra != obra);
    }
}

/// Guarda un informe del asistente (Markdown) y devuelve su ruta.
pub fn guardar_informe(carpeta: &Path, titulo: &str, contenido: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(carpeta)?;
    let base = nombre_fichero(titulo);
    let mut ruta = carpeta.join(format!("{base}.md"));
    let mut n = 2;
    while ruta.exists() {
        ruta = carpeta.join(format!("{base} ({n}).md"));
        n += 1;
    }
    escribir(&ruta, contenido)?;
    Ok(ruta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversacion_por_obra_con_pendientes() {
        let dir = std::env::temp_dir().join(format!("ppto-conv-{}", std::process::id()));
        let ruta = dir.join("asistente.json");
        let _ = std::fs::remove_file(&ruta);
        let id = Conversaciones::modificar(&ruta, |c| {
            c.anadir("A [1]", Autor::Usuario, "valora la clima", vec!["oferta.pdf".into()])
        })
        .unwrap();
        Conversaciones::modificar(&ruta, |c| c.anadir("B [2]", Autor::Usuario, "otra obra", vec![])).unwrap();
        let c = Conversaciones::cargar(&ruta);
        assert_eq!(c.pendientes("A [1]").len(), 1);
        assert_eq!(c.pendientes("A [1]")[0].adjuntos, ["oferta.pdf"]);

        Conversaciones::modificar(&ruta, |c| c.responder("A [1]", &[id], "hecho: 12 propuestas", None)).unwrap();
        let c = Conversaciones::cargar(&ruta);
        assert!(c.pendientes("A [1]").is_empty());
        assert_eq!(c.pendientes("B [2]").len(), 1);
        assert_eq!(c.de_obra("A [1]").len(), 2);
        assert_eq!(c.de_obra("A [1]")[1].autor, Autor::Asistente);

        let inf = guardar_informe(&dir.join("inf"), "Resumen: clima/PCI", "# Hola").unwrap();
        assert!(inf.file_name().unwrap().to_string_lossy().starts_with("Resumen_ clima_PCI"));
        let inf2 = guardar_informe(&dir.join("inf"), "Resumen: clima/PCI", "# Hola").unwrap();
        assert_ne!(inf, inf2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_se_sobrescribe_un_fichero_danado() {
        let dir = std::env::temp_dir().join(format!("ppto-conv-d-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ruta = dir.join("asistente.json");
        std::fs::write(&ruta, "{ esto no es json").unwrap();
        assert!(Conversaciones::modificar(&ruta, |c| c.anadir("A", Autor::Usuario, "x", vec![])).is_err());
        assert_eq!(std::fs::read_to_string(&ruta).unwrap(), "{ esto no es json");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
