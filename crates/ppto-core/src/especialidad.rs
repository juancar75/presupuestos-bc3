// SPDX-License-Identifier: GPL-3.0-or-later
//! Especialidad de una obra (qué se presupuesta: ventilación de garaje, PCI,
//! climatización…). Orienta el programa: la plantilla de la que se parte y
//! las bases de precios que se ofrecen, para que quien empieza vea solo lo
//! de su especialidad y lo común.

use crate::plantillas::Plantilla;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Especialidad {
    /// Ventilación de garaje y de zonas comunes, detección de CO y
    /// ventilación de escaleras.
    VentilacionGaraje,
    /// Protección contra incendios.
    Pci,
    /// Climatización (RITE).
    Climatizacion,
    /// Ventilación de viviendas (VMC).
    VentilacionViviendas,
    /// Fontanería y ACS.
    Fontaneria,
    /// Electricidad.
    Electricidad,
}

impl Especialidad {
    pub const TODAS: [Especialidad; 6] = [
        Self::VentilacionGaraje,
        Self::Pci,
        Self::Climatizacion,
        Self::VentilacionViviendas,
        Self::Fontaneria,
        Self::Electricidad,
    ];

    pub fn nombre(self) -> &'static str {
        match self {
            Self::VentilacionGaraje => "Ventilación de garaje y zonas comunes",
            Self::Pci => "Protección contra incendios (PCI)",
            Self::Climatizacion => "Climatización",
            Self::VentilacionViviendas => "Ventilación de viviendas",
            Self::Fontaneria => "Fontanería y ACS",
            Self::Electricidad => "Electricidad",
        }
    }

    /// Una línea de lo que incluye.
    pub fn descripcion(self) -> &'static str {
        match self {
            Self::VentilacionGaraje => {
                "Ventiladores F400, conductos y rejillas, detección de CO, cuadros, trasteros y cuartos, escaleras"
            }
            Self::Pci => "Extintores, BIE, detección y alarma, rociadores, columna seca, señalización",
            Self::Climatizacion => "Producción, distribución, unidades terminales, control y legalización RITE",
            Self::VentilacionViviendas => "VMC de simple o doble flujo, bocas, conductos y extractores",
            Self::Fontaneria => "Acometida, contadores, distribución, ACS y saneamiento",
            Self::Electricidad => "Cuadros, líneas, canalizaciones y mecanismos",
        }
    }

    /// Texto estable para guardar (datos de la obra, ficheros de bases).
    pub fn clave(self) -> &'static str {
        match self {
            Self::VentilacionGaraje => "ventilacion_garaje",
            Self::Pci => "pci",
            Self::Climatizacion => "climatizacion",
            Self::VentilacionViviendas => "ventilacion_viviendas",
            Self::Fontaneria => "fontaneria",
            Self::Electricidad => "electricidad",
        }
    }

    pub fn leer(clave: &str) -> Option<Self> {
        Self::TODAS.into_iter().find(|e| e.clave() == clave.trim())
    }

    /// Plantilla de obra de la que se parte, si la hay.
    pub fn plantilla(self) -> Option<Plantilla> {
        match self {
            Self::VentilacionGaraje => Some(Plantilla::VentilacionGaraje),
            Self::Pci => Some(Plantilla::Pci),
            Self::Climatizacion => Some(Plantilla::Climatizacion),
            _ => None,
        }
    }

    /// Especialidad de una plantilla.
    pub fn de_plantilla(t: Plantilla) -> Self {
        match t {
            Plantilla::VentilacionGaraje => Self::VentilacionGaraje,
            Plantilla::Pci => Self::Pci,
            Plantilla::Climatizacion => Self::Climatizacion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claves_unicas_y_reversibles() {
        let mut vistas = std::collections::BTreeSet::new();
        for e in Especialidad::TODAS {
            assert!(vistas.insert(e.clave()));
            assert_eq!(Especialidad::leer(e.clave()), Some(e));
            assert!(!e.nombre().is_empty() && !e.descripcion().is_empty());
            if let Some(t) = e.plantilla() {
                assert_eq!(Especialidad::de_plantilla(t), e);
            }
        }
        assert_eq!(Especialidad::leer("otra"), None);
    }
}
