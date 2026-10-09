// SPDX-License-Identifier: GPL-3.0-or-later
//! # ppto-bc3
//!
//! Intercambio de presupuestos en formato FIEBDC-3 (BC3).
//!
//! Principio: **lectura tolerante, escritura estricta**. El lector nunca se
//! detiene por un registro defectuoso: lo anota como incidencia con su número
//! de línea y continúa, de modo que el usuario recibe siempre el presupuesto
//! más completo posible y una lista de lo que hay que revisar.
//!
//! Tras importar, el presupuesto se recalcula con las reglas de `ppto-core` y
//! se compara con los precios que declara el propio BC3 (`discrepancias`).
//! Es la herramienta para detectar errores del presupuesto de origen y
//! diferencias de criterio con el programa que lo generó.
//!
//! Correspondencia de registros y decisiones abiertas: `docs/bc3/matriz-registros.md`.

mod lector;

pub use lector::{importar, importar_fichero};

use ppto_core::{Decimal, Presupuesto};
use std::collections::BTreeMap;
use std::fmt;

/// Gravedad de una incidencia de importación.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gravedad {
    /// Información útil (p. ej. registros conservados sin interpretar).
    Info,
    /// El dato se ha importado con una interpretación que conviene revisar.
    Aviso,
    /// El dato era incorrecto; se ha omitido o sustituido.
    Error,
}

/// Problema detectado al leer el fichero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incidencia {
    pub gravedad: Gravedad,
    /// Línea del fichero donde empieza el registro (1 = primera). 0 si es global.
    pub linea: usize,
    /// Tipo de registro (`C`, `D`, `M`…), vacío si es global.
    pub registro: String,
    pub mensaje: String,
}

impl fmt::Display for Incidencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g = match self.gravedad {
            Gravedad::Info => "info",
            Gravedad::Aviso => "aviso",
            Gravedad::Error => "ERROR",
        };
        if self.linea > 0 && !self.registro.is_empty() {
            write!(f, "[{g}] línea {} (~{}): {}", self.linea, self.registro, self.mensaje)
        } else if self.linea > 0 {
            write!(f, "[{g}] línea {}: {}", self.linea, self.mensaje)
        } else {
            write!(f, "[{g}] {}", self.mensaje)
        }
    }
}

/// Datos del registro `~V`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cabecera {
    pub propiedad: String,
    pub version_formato: String,
    pub programa: String,
    pub rotulo: String,
    pub juego_caracteres: String,
}

/// Porcentajes del registro `~K` (en puntos: 13 = 13 %).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Porcentajes {
    pub costes_indirectos: Option<Decimal>,
    pub gastos_generales: Option<Decimal>,
    pub beneficio_industrial: Option<Decimal>,
    pub baja: Option<Decimal>,
    pub iva: Option<Decimal>,
}

/// Concepto cuyo precio recalculado no coincide con el declarado en el BC3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discrepancia {
    pub codigo: String,
    pub resumen: String,
    pub declarado: Decimal,
    pub calculado: Decimal,
}

impl Discrepancia {
    pub fn diferencia(&self) -> Decimal {
        self.calculado - self.declarado
    }
}

/// Resultado de una importación.
#[derive(Debug, Clone)]
pub struct Importacion {
    pub presupuesto: Presupuesto,
    pub cabecera: Cabecera,
    pub porcentajes: Porcentajes,
    /// Número de registros leídos por tipo.
    pub registros: BTreeMap<String, usize>,
    pub incidencias: Vec<Incidencia>,
    /// Precios recalculados que difieren del valor declarado en `~C`.
    pub discrepancias: Vec<Discrepancia>,
    /// PEM declarado por el BC3 (precio del concepto raíz), si lo trae.
    pub pem_declarado: Option<Decimal>,
}

impl Importacion {
    pub fn errores(&self) -> usize {
        self.incidencias
            .iter()
            .filter(|i| i.gravedad == Gravedad::Error)
            .count()
    }

    pub fn avisos(&self) -> usize {
        self.incidencias
            .iter()
            .filter(|i| i.gravedad == Gravedad::Aviso)
            .count()
    }
}

/// Error que impide importar (fichero ilegible o sin ningún concepto).
#[derive(Debug)]
pub enum ErrorBc3 {
    Lectura(std::io::Error),
    SinConceptos,
}

impl fmt::Display for ErrorBc3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lectura(e) => write!(f, "no se pudo leer el fichero: {e}"),
            Self::SinConceptos => write!(f, "el fichero no contiene ningún concepto (~C): ¿es un BC3?"),
        }
    }
}

impl std::error::Error for ErrorBc3 {}
