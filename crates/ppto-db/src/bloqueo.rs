// SPDX-License-Identifier: GPL-3.0-or-later
//! Aviso de **obra abierta por otra persona** (o en otra ventana).
//!
//! Al abrir una obra se deja junto a ella un fichero `<obra>.abierta` con
//! quién la tiene, en qué equipo y desde cuándo, y se renueva cada minuto.
//! Si al abrirla ya hay uno reciente de otra persona o ventana, el programa
//! avisa: si los dos guardan, se crean revisiones distintas y los cambios no
//! se juntan. Un aviso viejo (programa cerrado de golpe) se ignora.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Un aviso sin renovar en este tiempo es de un programa ya cerrado (segundos).
pub const CADUCIDAD: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ocupante {
    pub usuario: String,
    pub equipo: String,
    pub pid: u32,
    /// Segundos desde 1970.
    pub desde: u64,
    pub latido: u64,
}

impl Ocupante {
    pub fn yo() -> Self {
        let var = |k: &[&str]| {
            k.iter()
                .find_map(|k| std::env::var(k).ok())
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "?".into())
        };
        let t = ahora();
        Self {
            usuario: var(&["USERNAME", "USER"]),
            equipo: var(&["COMPUTERNAME", "HOSTNAME"]),
            pid: std::process::id(),
            desde: t,
            latido: t,
        }
    }

    /// Es esta misma ventana.
    pub fn soy_yo(&self) -> bool {
        let y = Self::yo();
        self.pid == y.pid && self.equipo == y.equipo && self.usuario == y.usuario
    }

    /// «Ana en PC-OFICINA» u «otra ventana de este equipo».
    pub fn quien(&self) -> String {
        let y = Self::yo();
        if self.usuario == y.usuario && self.equipo == y.equipo {
            "usted en otra ventana de este equipo".into()
        } else {
            format!("{} en el equipo {}", self.usuario, self.equipo)
        }
    }
}

pub fn ahora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Fichero del aviso de una obra.
pub fn ruta_aviso(obra: &Path) -> PathBuf {
    let mut n = obra.as_os_str().to_owned();
    n.push(".abierta");
    PathBuf::from(n)
}

fn leer(r: &Path) -> Option<Ocupante> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(r).ok()?).ok()?;
    Some(Ocupante {
        usuario: v["usuario"].as_str()?.to_owned(),
        equipo: v["equipo"].as_str()?.to_owned(),
        pid: u32::try_from(v["pid"].as_u64()?).ok()?,
        desde: v["desde"].as_u64()?,
        latido: v["latido"].as_u64()?,
    })
}

/// Quién tiene abierta la obra ahora mismo (si no es esta ventana).
pub fn ocupada(obra: &Path) -> Option<Ocupante> {
    leer(&ruta_aviso(obra)).filter(|o| !o.soy_yo() && ahora().saturating_sub(o.latido) < CADUCIDAD)
}

/// Aviso propio sobre una obra abierta; se quita al soltarlo.
#[derive(Debug)]
pub struct Aviso {
    pub obra: PathBuf,
    yo: Ocupante,
}

impl Aviso {
    /// Deja el aviso (si no se puede escribir, p. ej. carpeta de solo
    /// lectura, no pasa nada: solo no se avisará a otros).
    pub fn poner(obra: &Path) -> Self {
        let a = Self {
            obra: obra.to_owned(),
            yo: Ocupante::yo(),
        };
        a.escribir();
        a
    }

    fn escribir(&self) {
        let o = &self.yo;
        let t = json!({
            "usuario": o.usuario,
            "equipo": o.equipo,
            "pid": o.pid,
            "desde": o.desde,
            "latido": o.latido,
        });
        let _ = std::fs::write(ruta_aviso(&self.obra), t.to_string());
    }

    /// Renueva el aviso (llamar cada minuto).
    pub fn latir(&mut self) {
        self.yo.latido = ahora();
        self.escribir();
    }
}

impl Drop for Aviso {
    fn drop(&mut self) {
        let r = ruta_aviso(&self.obra);
        // Solo si sigue siendo el nuestro
        if leer(&r).is_some_and(|o| o.soy_yo()) {
            let _ = std::fs::remove_file(r);
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn aviso_de_obra_abierta() {
        let dir = std::env::temp_dir().join(format!("ppto-bloqueo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let obra = dir.join("obra.sqlite");
        assert_eq!(ruta_aviso(&obra), dir.join("obra.sqlite.abierta"));
        assert!(ocupada(&obra).is_none());
        {
            let mut a = Aviso::poner(&obra);
            // El propio no cuenta
            assert!(ocupada(&obra).is_none());
            a.latir();
            assert!(ruta_aviso(&obra).is_file());
        }
        assert!(!ruta_aviso(&obra).exists(), "se quita al cerrar");
        // Otro equipo, reciente: avisa
        let otro = json!({"usuario":"ana","equipo":"PC-OFICINA","pid":1,"desde":ahora(),"latido":ahora()});
        std::fs::write(ruta_aviso(&obra), otro.to_string()).unwrap();
        let o = ocupada(&obra).unwrap();
        assert_eq!(o.quien(), "ana en el equipo PC-OFICINA");
        // «Abrir igualmente»: el aviso pasa a ser el nuestro y al cerrar se quita
        drop(Aviso::poner(&obra));
        assert!(ocupada(&obra).is_none());
        // Viejo: se ignora
        let viejo = json!({"usuario":"ana","equipo":"PC","pid":1,"desde":0,"latido":ahora() - CADUCIDAD - 1});
        std::fs::write(ruta_aviso(&obra), viejo.to_string()).unwrap();
        assert!(ocupada(&obra).is_none());
        std::fs::write(ruta_aviso(&obra), "roto").unwrap();
        assert!(ocupada(&obra).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
