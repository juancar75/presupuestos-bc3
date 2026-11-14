// SPDX-License-Identifier: GPL-3.0-or-later
//! Intercambio con la aplicación independiente `legalizacion`.
//!
//! El presupuestador escribe una [`Solicitud`] (la obra entera y su margen),
//! lanza `legalizacion --solicitud <fichero> --respuesta <fichero>` y, cuando
//! el programa se cierra, lee la [`Respuesta`] (la partida calculada) y la
//! aplica con [`crate::aplicar`]. Las dos aplicaciones solo comparten estos
//! dos JSON: la de legalización también funciona sola, sin presupuesto.

use crate::{Calculo, Disciplina};
use ppto_core::Presupuesto;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Versión del formato; se sube si cambia de forma incompatible.
pub const VERSION_INTERCAMBIO: u32 = 1;

/// Lo que el presupuestador pasa a la aplicación de legalización.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Solicitud {
    pub version: u32,
    /// Obra de la que se sacan los capítulos (base del PEM) y las mediciones.
    pub presupuesto: Presupuesto,
    /// Sobrecoste de la obra (GG + BI, %) para la tarifa del rol instalador.
    pub sobrecoste: Decimal,
}

/// Lo que devuelve la aplicación al pulsar «Crear la partida».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Respuesta {
    pub version: u32,
    pub disciplina: Disciplina,
    pub calculo: Calculo,
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorIntercambio {
    #[error("no se pudo leer o escribir {0}: {1}")]
    Fichero(String, std::io::Error),
    #[error("formato no válido en {0}: {1}")]
    Formato(String, serde_json::Error),
    #[error("versión de intercambio {0} no admitida (se esperaba {VERSION_INTERCAMBIO})")]
    Version(u32),
}

fn escribir<T: Serialize>(ruta: &Path, v: &T) -> Result<(), ErrorIntercambio> {
    let nombre = ruta.display().to_string();
    let texto = serde_json::to_string(v).map_err(|e| ErrorIntercambio::Formato(nombre.clone(), e))?;
    std::fs::write(ruta, texto).map_err(|e| ErrorIntercambio::Fichero(nombre, e))
}

fn leer<T: for<'de> Deserialize<'de>>(ruta: &Path) -> Result<T, ErrorIntercambio> {
    let nombre = ruta.display().to_string();
    let texto = std::fs::read_to_string(ruta).map_err(|e| ErrorIntercambio::Fichero(nombre.clone(), e))?;
    serde_json::from_str(&texto).map_err(|e| ErrorIntercambio::Formato(nombre, e))
}

impl Solicitud {
    pub fn nueva(presupuesto: Presupuesto, sobrecoste: Decimal) -> Self {
        Self {
            version: VERSION_INTERCAMBIO,
            presupuesto,
            sobrecoste,
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), ErrorIntercambio> {
        escribir(ruta, self)
    }

    pub fn cargar(ruta: &Path) -> Result<Self, ErrorIntercambio> {
        let s: Self = leer(ruta)?;
        if s.version != VERSION_INTERCAMBIO {
            return Err(ErrorIntercambio::Version(s.version));
        }
        Ok(s)
    }
}

impl Respuesta {
    pub fn nueva(disciplina: Disciplina, calculo: Calculo) -> Self {
        Self {
            version: VERSION_INTERCAMBIO,
            disciplina,
            calculo,
        }
    }

    pub fn guardar(&self, ruta: &Path) -> Result<(), ErrorIntercambio> {
        escribir(ruta, self)
    }

    pub fn cargar(ruta: &Path) -> Result<Self, ErrorIntercambio> {
        let r: Self = leer(ruta)?;
        if r.version != VERSION_INTERCAMBIO {
            return Err(ErrorIntercambio::Version(r.version));
        }
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Baremo, Documento, Expediente, aplicar, calcular, codigo_partida};

    fn dir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("leg-intercambio-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn la_respuesta_se_aplica_igual_que_el_calculo_original() {
        let d = dir();
        let b = Baremo::ejemplo(2026);
        let mut e = Expediente::nuevo(Disciplina::Rite, Documento::Memoria, Decimal::from(50_000));
        e.potencia_calor_kw = Decimal::from(20);
        let c = calcular(&b, &e);

        let ruta = d.join("respuesta.json");
        Respuesta::nueva(Disciplina::Rite, c.clone()).guardar(&ruta).unwrap();
        let r = Respuesta::cargar(&ruta).unwrap();
        assert_eq!(r.calculo, c);

        // La partida creada desde el JSON es idéntica a la creada en memoria.
        let mut p1 = Presupuesto::new("Obra", "OBRA", "Obra");
        let mut p2 = p1.clone();
        aplicar(&mut p1, Disciplina::Rite, &c).unwrap();
        aplicar(&mut p2, r.disciplina, &r.calculo).unwrap();
        assert_eq!(p1, p2);
        assert!(p2.conceptos.contains_key(&codigo_partida(Disciplina::Rite)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_solicitud_conserva_la_obra_y_rechaza_otra_version() {
        let d = dir();
        let ruta = d.join("solicitud.json");
        let s = Solicitud::nueva(Presupuesto::new("Obra", "OBRA", "Obra"), Decimal::from(19));
        s.guardar(&ruta).unwrap();
        assert_eq!(Solicitud::cargar(&ruta).unwrap(), s);

        let mut otra = s.clone();
        otra.version = VERSION_INTERCAMBIO + 1;
        otra.guardar(&ruta).unwrap();
        assert!(matches!(Solicitud::cargar(&ruta), Err(ErrorIntercambio::Version(_))));
        let _ = std::fs::remove_dir_all(&d);
    }
}
