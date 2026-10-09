// SPDX-License-Identifier: GPL-3.0-or-later
//! Explosión de recursos: cantidades totales de mano de obra, materiales,
//! maquinaria y subcontratas necesarias para ejecutar el presupuesto.
//! Base de los informes 6–10 y 14 del catálogo (P-015) y de las horas por oficio.
//!
//! Regla: cantidad del recurso = Σ (cantidad de la partida en obra ×
//! rendimiento en el descompuesto, a través de los precios auxiliares);
//! importe = cantidad total × precio del recurso, redondeado a `importe`.
//! Las líneas porcentuales se agrupan como pseudo-recurso con su importe.
//!
//! Como el PEM se calcula con importes de línea redondeados y la explosión
//! multiplica cantidades totales, ambos pueden diferir unos céntimos. La
//! diferencia se informa como `descuadre_redondeo`, nunca se oculta.

use crate::concepto::Naturaleza;
use crate::decimales::redondear;
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, PartialEq)]
pub struct RecursoExplotado {
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub naturaleza: Naturaleza,
    pub cantidad: Decimal,
    pub precio: Decimal,
    pub importe: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Explosion {
    pub recursos: Vec<RecursoExplotado>,
    pub total: Decimal,
    /// Coste directo de la obra (PEM sin costes indirectos).
    pub coste_directo: Decimal,
    /// Coste directo − total de la explosión.
    pub descuadre_redondeo: Decimal,
}

impl Explosion {
    pub fn por_naturaleza(&self, n: Naturaleza) -> impl Iterator<Item = &RecursoExplotado> {
        self.recursos.iter().filter(move |r| r.naturaleza == n)
    }
    pub fn recurso(&self, codigo: &str) -> Option<&RecursoExplotado> {
        self.recursos.iter().find(|r| r.codigo == codigo)
    }
}

impl Presupuesto {
    pub fn explosion_recursos(&self) -> Result<Explosion, ErrorMotor> {
        self.validar()?;
        let mut cantidades: BTreeMap<String, Decimal> = BTreeMap::new();
        let mut importes_pct: BTreeMap<String, Decimal> = BTreeMap::new();
        let mut cache = HashMap::new();
        for (partida, cant) in self.cantidades_partidas()? {
            self.explotar(&partida, cant, &mut cantidades, &mut importes_pct, &mut cache)?;
        }
        let d = &self.decimales;
        let mut recursos = Vec::new();
        for (codigo, cantidad) in cantidades {
            let c = self.concepto(&codigo)?;
            let precio = redondear(c.precio, d.precio);
            recursos.push(RecursoExplotado {
                codigo,
                unidad: c.unidad.clone(),
                resumen: c.resumen.clone(),
                naturaleza: c.naturaleza,
                cantidad,
                precio,
                importe: redondear(cantidad * precio, d.importe),
            });
        }
        for (codigo, importe) in importes_pct {
            let c = self.concepto(&codigo)?;
            recursos.push(RecursoExplotado {
                codigo,
                unidad: c.unidad.clone(),
                resumen: c.resumen.clone(),
                naturaleza: Naturaleza::Porcentaje,
                cantidad: Decimal::ZERO,
                precio: Decimal::ZERO,
                importe: redondear(importe, d.importe),
            });
        }
        let total: Decimal = recursos.iter().map(|r| r.importe).sum();
        let coste_directo = self.coste_directo()?;
        Ok(Explosion {
            recursos,
            total,
            coste_directo,
            descuadre_redondeo: coste_directo - total,
        })
    }

    fn explotar(
        &self,
        codigo: &str,
        mult: Decimal,
        cantidades: &mut BTreeMap<String, Decimal>,
        importes_pct: &mut BTreeMap<String, Decimal>,
        cache: &mut HashMap<String, Decimal>,
    ) -> Result<(), ErrorMotor> {
        let c = self.concepto(codigo)?;
        if c.naturaleza.es_basico() {
            *cantidades.entry(codigo.to_owned()).or_insert(Decimal::ZERO) += mult;
            return Ok(());
        }
        for lv in self.lineas_rec(codigo, cache)? {
            match lv.naturaleza {
                Naturaleza::Porcentaje => {
                    *importes_pct.entry(lv.hijo).or_insert(Decimal::ZERO) += mult * lv.importe;
                }
                _ => self.explotar(&lv.hijo, mult * lv.cantidad, cantidades, importes_pct, cache)?,
            }
        }
        Ok(())
    }
}
