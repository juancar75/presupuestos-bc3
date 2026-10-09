// SPDX-License-Identifier: GPL-3.0-or-later
//! Paquetes de trabajo y escenarios de subcontratación (P-016, P-017).
//!
//! Un **paquete** agrupa recursos de una o varias partidas (normalmente la
//! mano de obra propia) que pasan a ejecutarse por un subcontratista con su
//! propia unidad de contratación: por unidad (m², ml, ud…) o importe alzado.
//!
//! Garantías del escenario:
//! 1. El presupuesto de origen no se modifica (se recibe por referencia
//!    inmutable): los costes originales quedan intactos.
//! 2. Sin duplicidad: cada recurso de cada partida solo puede sustituirlo un
//!    paquete; si dos lo reclaman, el escenario es un error.
//! 3. Trazabilidad: por cada asignación se informa del coste retirado, el
//!    coste contratado y las horas propias liberadas.
//!
//! Criterio (a validar en revisión técnica): las líneas porcentuales del
//! descompuesto original (p. ej. medios auxiliares) **no** se recalculan al
//! retirar recursos. El escenario lo indica en `notas`.

use crate::concepto::Naturaleza;
use crate::decimales::redondear;
use crate::error::ErrorMotor;
use crate::presupuesto::Presupuesto;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Contratacion {
    /// Precio por unidad de contratación (m², ml, ud, h…).
    PorUnidad { unidad: String, precio: Decimal },
    /// Importe cerrado; se reparte entre asignaciones en proporción al coste retirado.
    Alzado { importe: Decimal },
}

/// Vinculación de un paquete con una partida.
#[derive(Debug, Clone, PartialEq)]
pub struct Asignacion {
    pub partida: String,
    /// Recursos de la descomposición directa de la partida que se sustituyen.
    pub recursos: Vec<String>,
    /// Unidades de contratación por unidad de la partida
    /// (p. ej. 7 ml de tubo por m² de suelo radiante). Ignorado en alzados.
    pub factor_conversion: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaqueteTrabajo {
    pub codigo: String,
    pub descripcion: String,
    pub contratacion: Contratacion,
    pub asignaciones: Vec<Asignacion>,
    pub incluye: Vec<String>,
    pub excluye: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetalleAsignacion {
    pub paquete: String,
    pub partida: String,
    pub cantidad_partida: Decimal,
    pub cantidad_contratada: Decimal,
    pub coste_retirado: Decimal,
    pub coste_contratado: Decimal,
    /// Horas (u otra unidad) liberadas por recurso de mano de obra.
    pub horas_liberadas: BTreeMap<String, Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultadoEscenario {
    pub pem_original: Decimal,
    pub coste_retirado: Decimal,
    pub coste_contratado: Decimal,
    pub pem_escenario: Decimal,
    /// Positivo = ahorro respecto al original.
    pub ahorro: Decimal,
    pub horas_liberadas: BTreeMap<String, Decimal>,
    pub detalle: Vec<DetalleAsignacion>,
    pub notas: Vec<String>,
}

impl ResultadoEscenario {
    /// Precio equivalente por unidad de partida de un paquete, para comparar
    /// ofertas con distintas unidades de contratación (P-019).
    pub fn precio_equivalente(&self, paquete: &str, partida: &str, decimales: u32) -> Option<Decimal> {
        self.detalle
            .iter()
            .find(|d| d.paquete == paquete && d.partida == partida && !d.cantidad_partida.is_zero())
            .map(|d| redondear(d.coste_contratado / d.cantidad_partida, decimales))
    }
}

/// Simula la sustitución de recursos por subcontratación sin tocar `p`.
pub fn simular(p: &Presupuesto, paquetes: &[PaqueteTrabajo]) -> Result<ResultadoEscenario, ErrorMotor> {
    let dec = p.decimales;
    let pem = p.pem()?;
    let cantidades = p.cantidades_partidas()?;
    let mut reclamado: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut detalle = Vec::new();
    let mut notas = Vec::new();

    for paq in paquetes {
        let mut detalle_paq = Vec::new();
        for a in &paq.asignaciones {
            let qp = *cantidades
                .get(&a.partida)
                .ok_or_else(|| ErrorMotor::PartidaSinMedicion(a.partida.clone()))?;
            let lineas = p.lineas_valoradas(&a.partida)?;
            if lineas.iter().any(|l| l.naturaleza == Naturaleza::Porcentaje) {
                notas.push(format!(
                    "{}: las líneas porcentuales de «{}» se mantienen sobre la base original",
                    paq.codigo, a.partida
                ));
            }
            let mut retirado = Decimal::ZERO;
            let mut horas = BTreeMap::new();
            for r in &a.recursos {
                let clave = (a.partida.clone(), r.clone());
                if let Some(previo) = reclamado.get(&clave) {
                    return Err(ErrorMotor::DobleSustitucion {
                        partida: a.partida.clone(),
                        recurso: r.clone(),
                        paquete_previo: previo.clone(),
                    });
                }
                let linea = lineas
                    .iter()
                    .find(|l| &l.hijo == r)
                    .ok_or_else(|| ErrorMotor::RecursoNoEnPartida {
                        partida: a.partida.clone(),
                        recurso: r.clone(),
                    })?;
                reclamado.insert(clave, paq.codigo.clone());
                // Mismo criterio que el capítulo: cantidad de obra × importe unitario visible.
                retirado += redondear(qp * linea.importe, dec.importe);
                if linea.naturaleza == Naturaleza::ManoObra {
                    *horas.entry(r.clone()).or_insert(Decimal::ZERO) += qp * linea.cantidad;
                }
            }
            let (cantidad_contratada, contratado) = match &paq.contratacion {
                Contratacion::PorUnidad { precio, .. } => {
                    let q = redondear(qp * a.factor_conversion, dec.medicion);
                    (q, redondear(q * *precio, dec.importe))
                }
                Contratacion::Alzado { .. } => (Decimal::ZERO, Decimal::ZERO),
            };
            detalle_paq.push(DetalleAsignacion {
                paquete: paq.codigo.clone(),
                partida: a.partida.clone(),
                cantidad_partida: qp,
                cantidad_contratada,
                coste_retirado: retirado,
                coste_contratado: contratado,
                horas_liberadas: horas,
            });
        }
        if let Contratacion::Alzado { importe } = &paq.contratacion {
            repartir_alzado(&mut detalle_paq, *importe, dec.importe);
        }
        detalle.extend(detalle_paq);
    }

    let coste_retirado: Decimal = detalle.iter().map(|d| d.coste_retirado).sum();
    let coste_contratado: Decimal = detalle.iter().map(|d| d.coste_contratado).sum();
    let mut horas_liberadas = BTreeMap::new();
    for d in &detalle {
        for (k, v) in &d.horas_liberadas {
            *horas_liberadas.entry(k.clone()).or_insert(Decimal::ZERO) += *v;
        }
    }
    let pem_escenario = pem - coste_retirado + coste_contratado;
    Ok(ResultadoEscenario {
        pem_original: pem,
        coste_retirado,
        coste_contratado,
        pem_escenario,
        ahorro: pem - pem_escenario,
        horas_liberadas,
        detalle,
        notas,
    })
}

/// Reparto proporcional al coste retirado; el resto de redondeo va a la
/// última asignación para que la suma sea exactamente el alzado.
fn repartir_alzado(det: &mut [DetalleAsignacion], importe: Decimal, decimales: u32) {
    let base: Decimal = det.iter().map(|d| d.coste_retirado).sum();
    let n = det.len();
    let mut asignado = Decimal::ZERO;
    for (i, d) in det.iter_mut().enumerate() {
        d.coste_contratado = if i + 1 == n {
            importe - asignado
        } else if base.is_zero() {
            Decimal::ZERO
        } else {
            redondear(importe * d.coste_retirado / base, decimales)
        };
        asignado += d.coste_contratado;
    }
}
