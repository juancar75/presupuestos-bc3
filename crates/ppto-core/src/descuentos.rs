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

use crate::concepto::{Concepto, LineaDescomposicion, Naturaleza, PrecioTarifa};
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

/// Clave de la tabla de descuentos: «S&P» (general del fabricante) o
/// «S&P/E08» (grupo de producto).
pub fn clave(fabricante: &str, grupo: Option<&str>) -> String {
    let f = normalizar(fabricante);
    match grupo.map(normalizar).filter(|g| !g.is_empty()) {
        Some(g) => format!("{f}/{g}"),
        None => f,
    }
}

/// Separa una clave «FAB/GRUPO» en fabricante y grupo.
pub fn partes(clave: &str) -> (String, Option<String>) {
    match clave.split_once('/') {
        Some((f, g)) => (normalizar(f), Some(normalizar(g)).filter(|g| !g.is_empty())),
        None => (normalizar(clave), None),
    }
}

/// Una fila de la tabla de descuentos.
#[derive(Debug, Clone, PartialEq)]
pub struct Fabricante {
    /// Clave («S&P» o «S&P/E08»).
    pub nombre: String,
    pub fabricante: String,
    pub grupo: Option<String>,
    pub descuento: Decimal,
    /// Recursos a los que se aplica este descuento.
    pub recursos: usize,
}

impl Presupuesto {
    /// Descuento general del fabricante (0 si no tiene).
    pub fn descuento(&self, fabricante: &str) -> Decimal {
        self.descuentos
            .get(&normalizar(fabricante))
            .copied()
            .unwrap_or_default()
    }

    /// Clave del descuento que se aplica a una tarifa: la del grupo si
    /// tiene descuento propio; si no, la general del fabricante.
    pub fn clave_aplicada(&self, t: &PrecioTarifa) -> String {
        if let Some(g) = &t.grupo {
            let k = clave(&t.fabricante, Some(g));
            if self.descuentos.contains_key(&k) {
                return k;
            }
        }
        normalizar(&t.fabricante)
    }

    /// Descuento que se aplica a una tarifa (grupo o general).
    pub fn descuento_tarifa(&self, t: &PrecioTarifa) -> Decimal {
        self.descuentos
            .get(&self.clave_aplicada(t))
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

    /// Liga un recurso básico a la tarifa de un fabricante (sin grupo).
    pub fn fijar_tarifa(&mut self, codigo: &str, fabricante: &str, pvp: Decimal) -> Result<(), ErrorMotor> {
        self.fijar_tarifa_grupo(codigo, fabricante, None, pvp)
    }

    /// Liga un recurso básico a la tarifa de un fabricante y grupo de
    /// producto, y recalcula su neto con el descuento que le toca.
    pub fn fijar_tarifa_grupo(
        &mut self,
        codigo: &str,
        fabricante: &str,
        grupo: Option<&str>,
        pvp: Decimal,
    ) -> Result<(), ErrorMotor> {
        let fab = normalizar(fabricante);
        if fab.is_empty() || fab.contains('/') {
            return Err(ErrorMotor::ValorInvalido("fabricante no válido".into()));
        }
        if pvp < Decimal::ZERO {
            return Err(ErrorMotor::ValorInvalido("el PVP no puede ser negativo".into()));
        }
        let t = PrecioTarifa {
            fabricante: fab,
            grupo: grupo.map(normalizar).filter(|g| !g.is_empty()),
            pvp,
        };
        let neto = self.neto(pvp, self.descuento_tarifa(&t));
        let c = self.concepto_mut(codigo)?;
        if !c.naturaleza.es_basico() {
            return Err(ErrorMotor::ValorInvalido(format!(
                "«{codigo}» no es un recurso básico: no tiene precio de tarifa"
            )));
        }
        c.tarifa = Some(t);
        c.precio = neto;
        Ok(())
    }

    /// Cambia el grupo de producto de un recurso con tarifa (vacío = sin grupo).
    pub fn fijar_grupo(&mut self, codigo: &str, grupo: &str) -> Result<(), ErrorMotor> {
        let t = self
            .concepto(codigo)?
            .tarifa
            .clone()
            .ok_or_else(|| ErrorMotor::ValorInvalido(format!("«{codigo}» no tiene tarifa")))?;
        self.fijar_tarifa_grupo(codigo, &t.fabricante, Some(grupo), t.pvp)
    }

    /// Recalcula el neto de todos los recursos con tarifa.
    fn recalcular_tarifas(&mut self) {
        let netos: Vec<(String, Decimal)> = self
            .conceptos
            .values()
            .filter_map(|c| {
                c.tarifa
                    .as_ref()
                    .map(|t| (c.codigo.clone(), self.neto(t.pvp, self.descuento_tarifa(t))))
            })
            .collect();
        for (c, n) in netos {
            if let Some(x) = self.conceptos.get_mut(&c) {
                x.precio = n;
            }
        }
    }

    fn aplicados(&self, k: &str) -> Vec<String> {
        self.conceptos
            .values()
            .filter(|c| c.tarifa.as_ref().is_some_and(|t| self.clave_aplicada(t) == k))
            .map(|c| c.codigo.clone())
            .collect()
    }

    /// Fija el descuento de un fabricante («S&P») o de un grupo de producto
    /// («S&P/E08») y recalcula. Devuelve los recursos a los que se aplica.
    pub fn fijar_descuento(&mut self, clave_dto: &str, pct: Decimal) -> Result<Vec<String>, ErrorMotor> {
        validar_pct(pct)?;
        let (f, g) = partes(clave_dto);
        if f.is_empty() {
            return Err(ErrorMotor::ValorInvalido("falta el fabricante".into()));
        }
        let k = clave(&f, g.as_deref());
        self.descuentos.insert(k.clone(), pct);
        self.recalcular_tarifas();
        Ok(self.aplicados(&k))
    }

    /// Quita un descuento (los recursos de un grupo vuelven al general del
    /// fabricante; los del fabricante, a 0 %).
    pub fn quitar_descuento(&mut self, clave_dto: &str) -> Vec<String> {
        let (f, g) = partes(clave_dto);
        self.descuentos.remove(&clave(&f, g.as_deref()));
        let antes: Vec<String> = self
            .conceptos
            .values()
            .filter(|c| c.tarifa.as_ref().is_some_and(|t| t.fabricante == f))
            .map(|c| c.codigo.clone())
            .collect();
        self.recalcular_tarifas();
        antes
    }

    /// Tabla de descuentos: los definidos y los fabricantes con recursos.
    pub fn fabricantes(&self) -> Vec<Fabricante> {
        let fila = |k: &str, d: Decimal| {
            let (f, g) = partes(k);
            Fabricante {
                nombre: k.to_owned(),
                fabricante: f,
                grupo: g,
                descuento: d,
                recursos: 0,
            }
        };
        let mut m: BTreeMap<String, Fabricante> =
            self.descuentos.iter().map(|(k, d)| (k.clone(), fila(k, *d))).collect();
        for c in self.conceptos.values() {
            if let Some(t) = &c.tarifa {
                let k = self.clave_aplicada(t);
                m.entry(k.clone()).or_insert_with(|| fila(&k, Decimal::ZERO)).recursos += 1;
            }
        }
        m.into_values().collect()
    }

    /// Copia del presupuesto con los descuentos **a la vista** en los
    /// descompuestos: cada recurso con tarifa y descuento pasa a su PVP y,
    /// detrás de cada línea suya, se añade una línea porcentual negativa
    /// «Descuento FABRICANTE −x %» cuya máscara es el código del recurso
    /// (`<código>%`), de modo que solo se aplica a él. Es lo que se exporta al
    /// BC3 para que Presto muestre PVP y descuento.
    ///
    /// Los recursos que no se pueden mostrar así se dejan a neto y se avisa:
    /// cuelgan directamente de un capítulo, su código tiene «%» o «&» o pasa
    /// de 19 caracteres, u otra línea del mismo descompuesto empieza igual.
    /// El importe puede diferir algún céntimo del neto por el redondeo de la
    /// línea porcentual.
    pub fn con_descuentos_visibles(&self) -> (Presupuesto, Vec<String>) {
        self.con_descuentos_visibles_hasta(20)
    }

    /// Como [`Self::con_descuentos_visibles`], con códigos de `max` caracteres
    /// como mucho: la línea `<código>%` tiene que caber.
    pub fn con_descuentos_visibles_hasta(&self, max: usize) -> (Presupuesto, Vec<String>) {
        let mut p = self.clone();
        let mut avisos = Vec::new();
        // Recursos con descuento > 0 y si se pueden mostrar.
        let mut candidatos: BTreeMap<String, (Decimal, String)> = BTreeMap::new();
        for c in self.conceptos.values() {
            let Some(t) = &c.tarifa else { continue };
            let d = self.descuento_tarifa(t);
            if d.is_zero() {
                continue;
            }
            if c.codigo.contains(['%', '&']) || c.codigo.chars().count() + 1 > max {
                avisos.push(format!(
                    "«{}» se exporta a neto: su código no admite línea de descuento (tiene % o & o pasa de {} caracteres)",
                    c.codigo,
                    max.saturating_sub(1)
                ));
                continue;
            }
            candidatos.insert(c.codigo.clone(), (d, self.clave_aplicada(t)));
        }
        // No se puede si cuelga de un capítulo o si la máscara alcanza a otra línea.
        for c in self.conceptos.values() {
            for l in &c.descomposicion {
                if !candidatos.contains_key(&l.hijo) {
                    continue;
                }
                let choca = c
                    .descomposicion
                    .iter()
                    .any(|o| o.hijo != l.hijo && o.hijo.starts_with(&l.hijo));
                if c.naturaleza == Naturaleza::Capitulo || choca {
                    avisos.push(format!(
                        "«{}» se exporta a neto: {}",
                        l.hijo,
                        if choca {
                            format!("en «{}» otra línea empieza por el mismo código", c.codigo)
                        } else {
                            format!("cuelga directamente del capítulo «{}»", c.codigo)
                        }
                    ));
                    candidatos.remove(&l.hijo);
                }
            }
        }
        // Solo los que se usan en algún descompuesto (sin conceptos huérfanos).
        candidatos.retain(|k, _| {
            self.conceptos
                .values()
                .any(|c| c.descomposicion.iter().any(|l| &l.hijo == k))
        });
        if candidatos.is_empty() {
            return (p, avisos);
        }
        // Recursos a PVP y una línea de descuento por recurso.
        for (codigo, (d, clave)) in &candidatos {
            if let Some(c) = p.conceptos.get_mut(codigo)
                && let Some(t) = &c.tarifa
            {
                c.precio = t.pvp;
            }
            let pc = format!("{codigo}%");
            let resumen = format!("Descuento {clave} {} %", d.normalize());
            p.conceptos.insert(pc.clone(), Concepto::porcentaje(pc, resumen));
        }
        for c in p.conceptos.values_mut() {
            if c.naturaleza == Naturaleza::Capitulo
                || !c.descomposicion.iter().any(|l| candidatos.contains_key(&l.hijo))
            {
                continue;
            }
            let mut nuevas = Vec::with_capacity(c.descomposicion.len() + 2);
            for l in c.descomposicion.drain(..) {
                let dto = candidatos.get(&l.hijo).map(|x| x.0);
                let pc = format!("{}%", l.hijo);
                nuevas.push(l);
                if let Some(d) = dto {
                    nuevas.push(LineaDescomposicion::new(pc, -d));
                }
            }
            c.descomposicion = nuevas;
        }
        (p, avisos)
    }
}
