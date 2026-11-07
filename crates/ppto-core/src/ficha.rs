// SPDX-License-Identifier: GPL-3.0-or-later
//! Ficha de datos del edificio: lo que se sabe de la obra (bloques, plantas,
//! plazas de garaje, superficies, escaleras…) para proponer y comprobar la
//! medición con reglas por especialidad. Cada especialidad pide sus campos;
//! los valores se guardan con la obra por su clave.

use crate::especialidad::Especialidad;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Un dato de la ficha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Campo {
    /// Clave estable (para guardar y para las reglas).
    pub clave: &'static str,
    pub etiqueta: &'static str,
    /// Unidad («ud», «m²», «m»…).
    pub unidad: &'static str,
    /// Grupo en la ficha («Edificio», «Garaje», «Comunes»…).
    pub grupo: &'static str,
}

const fn c(clave: &'static str, etiqueta: &'static str, unidad: &'static str, grupo: &'static str) -> Campo {
    Campo {
        clave,
        etiqueta,
        unidad,
        grupo,
    }
}

const EDIFICIO: &[Campo] = &[
    c("bloques", "Bloques o portales", "ud", "Edificio"),
    c("viviendas", "Viviendas (total)", "ud", "Edificio"),
    c("viviendas_planta", "Viviendas por planta", "ud", "Edificio"),
    c("plantas_sobre", "Plantas sobre rasante", "ud", "Edificio"),
    c("plantas_bajo", "Plantas bajo rasante", "ud", "Edificio"),
    c("altura_evacuacion", "Altura de evacuación", "m", "Edificio"),
    c("altura_cubierta", "Altura hasta cubierta", "m", "Edificio"),
];

const GARAJE: &[Campo] = &[
    c(
        "garaje_superficie_planta",
        "Superficie de garaje por planta",
        "m²",
        "Garaje",
    ),
    c("garaje_plazas", "Plazas de garaje (total)", "ud", "Garaje"),
    c("garaje_altura", "Altura libre del garaje", "m", "Garaje"),
    c(
        "garaje_sectores_planta",
        "Sectores de incendio por planta",
        "ud",
        "Garaje",
    ),
    c("garaje_rampas", "Rampas", "ud", "Garaje"),
    c(
        "garaje_distancia_cubierta",
        "Distancia del garaje a la cubierta o a la descarga",
        "m",
        "Garaje",
    ),
];

const COMUNES: &[Campo] = &[
    c("escaleras", "Escaleras", "ud", "Comunes"),
    c("vestibulos", "Vestíbulos de independencia", "ud", "Comunes"),
    c("trasteros", "Trasteros", "ud", "Comunes"),
    c("trasteros_superficie", "Superficie de trasteros", "m²", "Comunes"),
    c("cuartos_basuras", "Cuartos de basuras", "ud", "Comunes"),
    c("cuartos_instalaciones", "Cuartos de instalaciones", "ud", "Comunes"),
    c("locales", "Locales comerciales", "ud", "Comunes"),
    c("locales_superficie", "Superficie de locales", "m²", "Comunes"),
];

const PCI: &[Campo] = &[
    c("superficie_planta", "Superficie construida por planta", "m²", "PCI"),
    c("escaleras_presurizadas", "Escaleras con sobrepresión", "ud", "PCI"),
    c("columnas_secas", "Columnas secas", "ud", "PCI"),
];

const VIVIENDA: &[Campo] = &[
    c(
        "vivienda_superficie",
        "Superficie útil media por vivienda",
        "m²",
        "Vivienda",
    ),
    c("vivienda_dormitorios", "Dormitorios por vivienda", "ud", "Vivienda"),
    c("vivienda_banos", "Baños y aseos por vivienda", "ud", "Vivienda"),
    c(
        "vivienda_unidades",
        "Unidades interiores por vivienda",
        "ud",
        "Vivienda",
    ),
    c(
        "distancia_maquinas",
        "Distancia a cubierta o zona de máquinas",
        "m",
        "Vivienda",
    ),
];

/// Campos que pide una especialidad (sin especialidad, los del edificio).
pub fn campos(e: Option<Especialidad>) -> Vec<Campo> {
    let mut v: Vec<Campo> = EDIFICIO.to_vec();
    match e {
        Some(Especialidad::VentilacionGaraje) => {
            v.extend_from_slice(GARAJE);
            v.extend_from_slice(COMUNES);
        }
        Some(Especialidad::Pci) => {
            v.extend_from_slice(PCI);
            v.extend_from_slice(GARAJE);
            v.extend_from_slice(COMUNES);
        }
        Some(Especialidad::Climatizacion | Especialidad::VentilacionViviendas) => v.extend_from_slice(VIVIENDA),
        Some(Especialidad::Fontaneria) => {
            v.extend_from_slice(VIVIENDA);
            v.extend_from_slice(COMUNES);
        }
        Some(Especialidad::Electricidad) => {
            v.extend_from_slice(GARAJE);
            v.extend_from_slice(COMUNES);
        }
        None => {}
    }
    v
}

/// Valores de la ficha por clave.
pub type Ficha = BTreeMap<String, Decimal>;

/// Prefijo con el que la ficha va en los datos de la obra.
pub const PREFIJO: &str = "ficha.";

/// Ficha → datos de la obra (`ficha.<clave>` = valor).
pub fn a_datos(f: &Ficha, datos: &mut BTreeMap<String, String>) {
    for (k, v) in f {
        datos.insert(format!("{PREFIJO}{k}"), v.normalize().to_string());
    }
}

/// Datos de la obra → ficha.
pub fn de_datos(datos: &BTreeMap<String, String>) -> Ficha {
    datos
        .iter()
        .filter_map(|(k, v)| Some((k.strip_prefix(PREFIJO)?.to_owned(), v.parse::<Decimal>().ok()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn campos_sin_repetir_y_ida_y_vuelta() {
        for e in Especialidad::TODAS.map(Some).into_iter().chain([None]) {
            let v = campos(e);
            let claves: std::collections::BTreeSet<_> = v.iter().map(|c| c.clave).collect();
            assert_eq!(claves.len(), v.len(), "{e:?}");
        }
        assert!(
            campos(Some(Especialidad::VentilacionGaraje))
                .iter()
                .any(|c| c.clave == "garaje_plazas")
        );
        let f: Ficha = [
            ("garaje_plazas".to_owned(), dec!(60)),
            ("garaje_altura".to_owned(), dec!(2.70)),
        ]
        .into();
        let mut d = BTreeMap::from([("gg".to_owned(), "13".to_owned())]);
        a_datos(&f, &mut d);
        assert_eq!(d["ficha.garaje_altura"], "2.7");
        let g = de_datos(&d);
        assert_eq!(g.len(), 2);
        assert_eq!(g["garaje_plazas"], dec!(60));
    }
}
