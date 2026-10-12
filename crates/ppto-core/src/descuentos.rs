// SPDX-License-Identifier: GPL-3.0-or-later
//! Descuentos de fabricante sobre tarifa.
//!
//! Un recurso puede guardar su **PVP de tarifa** y su **fabricante**
//! ([`PrecioTarifa`]). Su precio neto es `PVP × (100 − descuento) / 100`,
//! redondeado a los decimales de precio, con el descuento del fabricante
//! en el presupuesto (p. ej. KOOLAIR 45 %). Al cambiar el descuento de un
//! fabricante se recalculan todos sus recursos de una vez.
//!
//! El precio neto se guarda en `precio`, de modo que el resto del motor,
//! los informes y el BC3 (que solo lleva netos) no cambian.

use crate::concepto::PrecioTarifa;
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use crate::redondear;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Nombre normalizado del fabricante (mayúsculas, sin espacios sobrantes).
pub fn normalizar(fabricante: &str) -> String {
    fabricante
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn validar_pct(pct: Decimal) -> Result<(), ErrorMotor> {
    if pct < Decimal::ZERO || pct >= Decimal::ONE_HUNDRED {
        return Err(ErrorMotor::ValorInvalido(
            "el descuento debe estar entre 0 y 99,99 %".into(),
        ));
    }
    Ok(())
}

/// Resumen de un fabricante: descuento y número de recursos ligados.
#[derive(Debug, Clone, PartialEq)]
pub struct Fabricante {
    pub nombre: String,
    pub descuento: Decimal,
    pub recursos: usize,
}

impl Presupuesto {
    /// Descuento del fabricante (0 si no tiene).
    pub fn descuento(&self, fabricante: &str) -> Decimal {
        self.descuentos
            .get(&normalizar(fabricante))
            .copied()
            .unwrap_or_default()
    }

    /// Precio neto a partir de un PVP y un descuento.
    pub fn neto(&self, pvp: Decimal, descuento: Decimal) -> Decimal {
        redondear(
            pvp * (Decimal::ONE_HUNDRED - descuento) / Decimal::ONE_HUNDRED,
            self.decimales.precio,
        )
    }

    /// Liga un recurso básico a la tarifa de un fabricante y recalcula su
    /// precio neto con el descuento actual del fabricante.
    pub fn fijar_tarifa(&mut self, codigo: &str, fabricante: &str, pvp: Decimal) -> Result<(), ErrorMotor> {
        let fab = normalizar(fabricante);
        if fab.is_empty() {
            return Err(ErrorMotor::ValorInvalido("falta el fabricante".into()));
        }
        if pvp < Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido("el PVP no puede ser negativo".into()));
        }
        let neto = self.neto(pvp, self.descuento(&fab));
        let c = self.concepto_mut(codigo)?;
        if !c.naturaleza.es_basico() {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» no es un recurso básico: no tiene precio de tarifa"
            )));
        }
        c.tarifa = Some(PrecioTarifa { fabricante: fab, pvp });
        c.precio = neto;
        Ok(())
    }

    /// Cambia el descuento de un fabricante y recalcula sus recursos.
    /// Devuelve los recursos recalculados.
    pub fn fijar_descuento(&mut self, fabricante: &str, pct: Decimal) -> Result<Vec<String>, ErrorMotor> {
        validar_pct(pct)?;
        let fab = normalizar(fabricante);
        if fab.is_empty() {
            return Err(ErrorMotor::ValorInvalido("falta el fabricante".into()));
        }
        self.descuentos.insert(fab.clone(), pct);
        let dec = self.decimales.precio;
        let mut cambiados = Vec::new();
        for c in self.conceptos.values_mut() {
            if let Some(t) = &c.tarifa
                && t.fabricante == fab
            {
                c.precio = redondear(t.pvp * (Decimal::ONE_HUNDRED - pct) / Decimal::ONE_HUNDRED, dec);
                cambiados.push(c.codigo.clone());
            }
        }
        Ok(cambiados)
    }

    /// Fabricantes con descuento o con recursos ligados.
    pub fn fabricantes(&self) -> Vec<Fabricante> {
        let mut m: BTreeMap<String, Fabricante> = self
            .descuentos
            .iter()
            .map(|(n, d)| {
                (
                    n.clone(),
                    Fabricante {
                        nombre: n.clone(),
                        descuento: *d,
                        recursos: 0,
                    },
                )
            })
            .collect();
        for c in self.conceptos.values() {
            if let Some(t) = &c.tarifa {
                m.entry(t.fabricante.clone())
                    .or_insert_with(|| Fabricante {
                        nombre: t.fabricante.clone(),
                        descuento: Decimal::ZERO,
                        recursos: 0,
                    })
                    .recursos += 1;
            }
        }
        m.into_values().collect()
    }
}
