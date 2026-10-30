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
//! - **`asistente.json`**: conversación con el asistente por obra
//!   (ver [`conversacion`]).
//!
//! Los ficheros se escriben de forma atómica (fichero temporal + renombrar)
//! y las modificaciones (leer, cambiar, guardar) se hacen con un fichero de
//! bloqueo para que el programa y el MCP no se pisen.

pub mod conversacion;

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

/// Ejecuta `f` con el fichero `ruta` bloqueado frente a otros procesos
/// (fichero `.lock` creado en exclusiva). Si otro proceso lo tiene, espera
/// hasta 3 s; un bloqueo de más de 30 s se considera abandonado.
pub fn con_bloqueo<R>(ruta: &Path, f: impl FnOnce() -> std::io::Result<R>) -> std::io::Result<R> {
    use std::time::{Duration, Instant};
    if let Some(d) = ruta.parent() {
        std::fs::create_dir_all(d)?;
    }
    let cerrojo = ruta.with_extension("lock");
    let inicio = Instant::now();
    loop {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&cerrojo) {
            Ok(_) => break,
            // En Windows, un cerrojo que otro proceso está borrando da
            // «acceso denegado» unos milisegundos: es lo mismo que ocupado.
            Err(e)
                if e.kind() == std::io::ErrorKind::AlreadyExists
                    || e.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                let abandonado = std::fs::metadata(&cerrojo)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|d| d > Duration::from_secs(30));
                if abandonado {
                    let _ = std::fs::remove_file(&cerrojo);
                } else if inicio.elapsed() > Duration::from_secs(3) {
                    return Err(std::io::Error::other(format!(
                        "{} está ocupado por otro proceso; vuelva a intentarlo",
                        ruta.display()
                    )));
                } else {
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
            Err(e) => return Err(e),
        }
    }
    let r = f();
    let _ = std::fs::remove_file(&cerrojo);
    r
}

/// Lee un JSON: si no existe, el valor por defecto; si no se puede leer o
/// está dañado, error (para no sobrescribirlo con un fichero vacío).
fn leer_estricto<T: Default + serde::de::DeserializeOwned>(ruta: &Path) -> std::io::Result<T> {
    match std::fs::read_to_string(ruta) {
        Ok(t) if t.trim().is_empty() => Ok(T::default()),
        Ok(t) => serde_json::from_str(&t).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} dañado: {e}", ruta.display()),
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

/// Escribe `texto` en `ruta` sin dejarla a medias (fichero temporal y
/// renombrado), reintentando si Windows la tiene abierta un instante.
pub fn escribir(ruta: &Path, texto: &str) -> std::io::Result<()> {
    if let Some(d) = ruta.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = ruta.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, texto)?;
    // Windows (o OneDrive, o el antivirus) puede tener el fichero abierto un
    // instante: se reintenta antes de dar error.
    let mut intento = 0;
    loop {
        match std::fs::rename(&tmp, ruta) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && intento < 40 => {
                intento += 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObraActiva {
    pub clave: String,
    /// Segundos desde 1970 de la última copia.
    pub guardada: u64,
    pub presupuesto: Presupuesto,
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

    /// Lee, modifica y guarda con el fichero bloqueado, para no pisar lo que
    /// haya escrito el otro proceso. Si la bandeja existe pero no se puede
    /// leer, no la toca (antes se sobrescribía con una vacía y se perdían
    /// las propuestas).
    pub fn modificar<R>(ruta: &Path, f: impl FnOnce(&mut Bandeja) -> R) -> std::io::Result<R> {
        con_bloqueo(ruta, || {
            let mut b: Bandeja = leer_estricto(ruta)?;
            let r = f(&mut b);
            b.guardar(ruta)?;
            Ok(r)
        })
    }
}
