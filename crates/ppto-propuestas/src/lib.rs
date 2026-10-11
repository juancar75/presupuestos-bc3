// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-propuestas
//!
//! Comunicación entre el programa de escritorio y el servidor MCP (que usa
//! una IA como Claude u otra compatible) a través de dos ficheros en la
//! carpeta de configuración del usuario:
//!
//! - **`obra-activa.json`**: copia del presupuesto que está abierto en el
//!   programa. La escribe el programa; el MCP la lee para que la IA vea las
//!   partidas (p. ej. las de un ciego sin precio).
//! - **`propuestas.json`**: bandeja de propuestas. El MCP añade propuestas
//!   (precio o descompuesto de una partida, con confianza y justificación);
//!   el programa las muestra y el usuario las acepta o rechaza. **La IA no
//!   modifica nunca el presupuesto directamente.**
//!
//! Los ficheros se escriben de forma atómica (fichero temporal + renombrar).

use ppto_core::Presupuesto;
use ppto_core::propuesta::{EstadoPropuesta, Propuesta};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Carpeta de configuración del programa (`%APPDATA%\presupuestos-bc3`,
/// en Linux `~/.config/presupuestos-bc3`).
pub fn carpeta_config() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("presupuestos-bc3"))
}

pub fn ruta_bandeja() -> Option<PathBuf> {
    carpeta_config().map(|c| c.join("propuestas.json"))
}

pub fn ruta_obra_activa() -> Option<PathBuf> {
    carpeta_config().map(|c| c.join("obra-activa.json"))
}

/// Identifica una obra entre el programa y el MCP.
pub fn clave(p: &Presupuesto) -> String {
    format!("{} [{}]", p.nombre, p.raiz)
}

fn ahora() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn escribir(ruta: &Path, texto: &str) -> std::io::Result<()> {
    if let Some(d) = ruta.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = ruta.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, texto)?;
    std::fs::rename(&tmp, ruta)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObraActiva {
    pub clave: String,
    /// Segundos desde 1970 de la última copia.
    pub guardada: u64,
    #[serde(with = "plano")]
    pub presupuesto: Presupuesto,
}

/// JSON no admite claves compuestas: las mediciones (padre, hijo) se
/// guardan como lista.
mod plano {
    use ppto_core::{Decimal, Decimales, Medicion, Presupuesto};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    #[derive(Serialize, Deserialize)]
    struct Plano {
        nombre: String,
        decimales: Decimales,
        raiz: String,
        conceptos: BTreeMap<String, ppto_core::Concepto>,
        mediciones: Vec<(String, String, Medicion)>,
        costes_indirectos: Decimal,
        opciones_ci: ppto_core::presupuesto::OpcionesCi,
    }

    pub fn serialize<S: Serializer>(p: &Presupuesto, s: S) -> Result<S::Ok, S::Error> {
        Plano {
            nombre: p.nombre.clone(),
            decimales: p.decimales,
            raiz: p.raiz.clone(),
            conceptos: p.conceptos.clone(),
            mediciones: p
                .mediciones
                .iter()
                .map(|((a, b), m)| (a.clone(), b.clone(), m.clone()))
                .collect(),
            costes_indirectos: p.costes_indirectos,
            opciones_ci: p.opciones_ci,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Presupuesto, D::Error> {
        let x = Plano::deserialize(d)?;
        Ok(Presupuesto {
            nombre: x.nombre,
            decimales: x.decimales,
            raiz: x.raiz,
            conceptos: x.conceptos,
            mediciones: x.mediciones.into_iter().map(|(a, b, m)| ((a, b), m)).collect(),
            costes_indirectos: x.costes_indirectos,
            opciones_ci: x.opciones_ci,
        })
    }
}

impl ObraActiva {
    pub fn guardar(p: &Presupuesto, ruta: &Path) -> std::io::Result<()> {
        let o = ObraActiva {
            clave: clave(p),
            guardada: ahora(),
            presupuesto: p.clone(),
        };
        let t = serde_json::to_string(&o).map_err(std::io::Error::other)?;
        escribir(ruta, &t)
    }

    pub fn cargar(ruta: &Path) -> Result<Self, String> {
        let t = std::fs::read_to_string(ruta).map_err(|e| {
            format!(
                "no hay obra activa ({e}): abra el presupuesto en el programa de escritorio ({})",
                ruta.display()
            )
        })?;
        serde_json::from_str(&t).map_err(|e| format!("obra activa dañada: {e}"))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entrada {
    /// [`clave`] de la obra.
    pub obra: String,
    /// Segundos desde 1970.
    pub creada: u64,
    pub propuesta: Propuesta,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bandeja {
    pub siguiente: u64,
    pub entradas: Vec<Entrada>,
}

impl Bandeja {
    /// Carga la bandeja; si no existe, vacía.
    pub fn cargar(ruta: &Path) -> Self {
        std::fs::read_to_string(ruta)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn guardar(&self, ruta: &Path) -> std::io::Result<()> {
        let t = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        escribir(ruta, &t)
    }

    /// Añade una propuesta pendiente (asigna el número). Si ya había otra
    /// pendiente para la misma partida, la sustituye.
    pub fn anadir(&mut self, obra: &str, mut p: Propuesta) -> u64 {
        self.siguiente = self
            .siguiente
            .max(self.entradas.iter().map(|e| e.propuesta.id).max().unwrap_or(0))
            + 1;
        p.id = self.siguiente;
        p.estado = EstadoPropuesta::Pendiente;
        self.entradas.retain(|e| {
            !(e.obra == obra && e.propuesta.partida == p.partida && e.propuesta.estado == EstadoPropuesta::Pendiente)
        });
        self.entradas.push(Entrada {
            obra: obra.to_owned(),
            creada: ahora(),
            propuesta: p,
        });
        self.siguiente
    }

    pub fn de_obra(&self, obra: &str) -> Vec<&Propuesta> {
        self.entradas
            .iter()
            .filter(|e| e.obra == obra)
            .map(|e| &e.propuesta)
            .collect()
    }

    pub fn pendientes(&self, obra: &str) -> Vec<&Propuesta> {
        self.de_obra(obra)
            .into_iter()
            .filter(|p| p.estado == EstadoPropuesta::Pendiente)
            .collect()
    }

    pub fn buscar(&self, id: u64) -> Option<&Propuesta> {
        self.entradas.iter().map(|e| &e.propuesta).find(|p| p.id == id)
    }

    /// Cambia el estado de una propuesta. Devuelve `false` si no existe.
    pub fn marcar(&mut self, id: u64, estado: EstadoPropuesta) -> bool {
        match self.entradas.iter_mut().find(|e| e.propuesta.id == id) {
            Some(e) => {
                e.propuesta.estado = estado;
                true
            }
            None => false,
        }
    }

    /// Lee, modifica y guarda (para no pisar lo que haya escrito el otro
    /// proceso entre medias).
    pub fn modificar<R>(ruta: &Path, f: impl FnOnce(&mut Bandeja) -> R) -> std::io::Result<R> {
        let mut b = Self::cargar(ruta);
        let r = f(&mut b);
        b.guardar(ruta)?;
        Ok(r)
    }
}
